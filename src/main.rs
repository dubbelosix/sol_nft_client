use {
    solana_client::{
        rpc_client::RpcClient,
        rpc_config::{ 
            RpcProgramAccountsConfig,
            RpcAccountInfoConfig,
        },
        rpc_filter::{
            RpcFilterType,
            Memcmp,
            MemcmpEncodedBytes,
            MemcmpEncoding,
        },
       rpc_request::TokenAccountsFilter,
    },
    solana_sdk::{
        account::Account,
        pubkey::Pubkey,
        commitment_config::{
            CommitmentConfig,
            CommitmentLevel,
        }
    },
    metaplex_token_metadata::state::Metadata,
    solana_account_decoder::{
        UiAccountEncoding,
        parse_token::UiTokenAccount,
    },
    solana_program::borsh::try_from_slice_unchecked,
    serde::{
        Serialize,
        Deserialize
    },
    serde_json::{Value,json},
    backoff::{
        retry,
        Error,
        ExponentialBackoff,
        ExponentialBackoffBuilder,
    },
    core::time::Duration,
    clap::{
        Arg,
        App,
        AppSettings,
        Parser
    },
    bs58,
    ureq,
    itertools::izip,
    
    std::{
        fs::File,
        io::{self, BufRead, LineWriter, Write},
        path::Path,
    },

    rayon::{
        prelude::*,
        iter::{ParallelIterator, IntoParallelRefIterator},
    },
    indicatif::{ProgressBar, ParallelProgressIterator},
};

const RPC_ENDPOINT: &str = "https://ssc-dao.genesysgo.net/";
const METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
const METADATA_CREATOR_ADDRESS_0_OFFSET: usize = 326;
const FAILED_MARKER: &str = "FAILED";

// setting this high because fetching tokens from the same creator_0 (candymachine) looks like it
// scans a lot of accounts on the rpc server side. It scans accounts matching a pattern in the
// metadata and can cause a timeout with default config. there has to be a better way to get this
// information (or for the rpc server to provide it). seems like it'll get worse as more token
// metadata accounts are created

const RPC_TIMEOUT: u64 = 300;

fn get_address_filter_for_program(address: &String, offset: usize) -> RpcProgramAccountsConfig {
    let memcmp = RpcFilterType::Memcmp(Memcmp{ 
        offset: offset,
        bytes: MemcmpEncodedBytes::Base58(address.to_string()),
        encoding: None,
    });

    RpcProgramAccountsConfig {
            filters: Some(vec![
                memcmp,
            ]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig{commitment:CommitmentLevel::Finalized}),
            },
            with_context: Some(false),
    }
}

fn get_struct_from_account_data(account_data: &Vec<u8>)-> Metadata {
    try_from_slice_unchecked(&account_data).unwrap()
}

fn get_mint_str_from_metadata(metadata: Metadata) -> String {
    bs58::encode(metadata.mint.to_bytes()).into_string()
}

#[derive(Debug,Serialize,Deserialize)]
struct GetTokenLargestAccounts {
    jsonrpc: String,
    id: u32,
    method: String,
    params: Vec<String>
}

#[derive(Debug,Serialize,Deserialize)]
struct GetTokenLargestAccountsResponse_Context {
    slot: u64
}

#[derive(Debug,Serialize,Deserialize)]
struct GetTokenLargestAccountsResponse_Result {
    context: GetTokenLargestAccountsResponse_Context,
    value : Vec<GetTokenLargestAccountsResponse_Value> 
}

#[derive(Debug,Serialize,Deserialize)]
struct GetTokenLargestAccountsResponse_Value {
    address: String,
    amount: String,
    decimals: u8,
    uiAmount: f64,
    uiAmountString: String,
}

#[derive(Debug,Serialize,Deserialize)]
struct GetTokenLargestAccountsResponse {
    jsonrpc: String,
    id: u32,
    result: GetTokenLargestAccountsResponse_Result,
}

fn get_token_account_for_mint(mint_addr_str: String)-> String {
    let request_body = GetTokenLargestAccounts {
        jsonrpc: "2.0".into(),
        id:1,
        method:"getTokenLargestAccounts".into(),
        params: vec![mint_addr_str]
    };

    let token_account_call = || {
        let raw_resp = ureq::post(RPC_ENDPOINT).send_json(&request_body);
        if let Ok(resp) = raw_resp {
            if let Ok(token_response) = resp.into_json::<GetTokenLargestAccountsResponse>() {
                for holder in token_response.result.value {
                    if holder.uiAmount == 1.0 {
                        return Ok(holder.address.into());
                    }
                }
            }
        }
        return Err(backoff::Error::Transient{err:"error",retry_after:None});
    };

    let backoff = ExponentialBackoffBuilder::default().with_max_elapsed_time(Some(Duration::from_secs(180))).build();
    match retry(backoff, token_account_call) {
        Ok(a) => a,
        _ => FAILED_MARKER.into()
    }
}

fn get_owner_of_assoc_token(client: &RpcClient, token_account: String) -> String {
    let token_account_owner_call = || {
        if token_account == FAILED_MARKER {
            return Err(backoff::Error::Permanent("error"));
        }
        let rpc_result = client.get_token_account_with_commitment(&Pubkey::new(&bs58::decode(&token_account).into_vec().unwrap())
                                                                  ,CommitmentConfig{commitment:CommitmentLevel::Finalized});

        if let Ok(response) = rpc_result {
            if let Some(ui_token_acc) = response.value{
                return Ok(ui_token_acc.owner); 
            } else {
                return Err(backoff::Error::Transient{err:"error",retry_after:None});
            }
        } else {
            return Err(backoff::Error::Transient{err:"error",retry_after:None});
        }
    };

    let backoff = ExponentialBackoffBuilder::default().with_max_elapsed_time(Some(Duration::from_secs(180))).build();
    match retry(backoff, token_account_owner_call) {
        Ok(a) => a,
        _ => FAILED_MARKER.into()
    }
}

fn get_list_of_mints_in_collection(client: &RpcClient, creator0_address: &String) -> Vec<String> {
    let metadata_program_pubkey: Pubkey = Pubkey::new(&bs58::decode(METADATA_PROGRAM).into_vec().unwrap());
    
    let result_vec = client.get_program_accounts_with_config(&metadata_program_pubkey,
                                                             get_address_filter_for_program(
                                                                 &creator0_address,
                                                                 METADATA_CREATOR_ADDRESS_0_OFFSET
                                                                 )
                                                             ).unwrap();
    
    let mut mints: Vec<String> = Vec::new();
    for (metadata_pda, account) in result_vec {
        let metadata = get_struct_from_account_data(&account.data);
        mints.push(get_mint_str_from_metadata(metadata));
    }
    mints
}

fn write_mint_info_to_file(creator_address: String, mintrows: Vec<(String,String,String)>) -> std::io::Result<String> {
    let filename = format!("{}.csv",creator_address);
    let path = Path::new(&filename);
    let file = File::create(&path)?;
    let mut file = LineWriter::new(file);
    file.write_all(b"Mint,Owner,Associated Token Account\n")?;

    for (mint_addr,owner,associated_token_addr) in mintrows {
        file.write_all(format!("{},{},{}\n",mint_addr,owner,associated_token_addr).as_bytes())?;
    }
    file.flush()?;

    Ok(filename.into())
}

fn retry_test_fn()-> Result<String,String> {
    let mut x = 1;
    let op = || {
        if x == 10 {
            println!("success");
            return Ok("rohan");
        } else {
            println!("error");
            x+=1;
            return Err(backoff::Error::Transient{err:"error",retry_after:None}); 
            //return Err("what");
        }
    };
    let backoff = ExponentialBackoff::default();
    retry(backoff, op);

    Ok("rohan".into())
}

struct FileState {
    succeeded: Vec<(String,String,String)>,
    failed: Vec<String>,
}

fn get_incomplete_mints(creator_address: &String)-> FileState {
    let fname = format!("{}.csv",creator_address);
    let mut fs = FileState {succeeded:Vec::<(String,String,String)>::new(), failed:Vec::<String>::new()};
    let path = Path::new(&fname);
    let file = File::open(path).expect(&format!("error opening file {}",path.display()));
    let lines = io::BufReader::new(file).lines();
    for line in lines {
        if let Ok(line_content) = line {
            let lc: Vec<&str> = line_content.split(",").collect();
            if lc[0] == "Mint" {
                continue;
            }
            if lc[1] == FAILED_MARKER || lc[2] == FAILED_MARKER {
                fs.failed.push(lc[0].into());
            } else {
                fs.succeeded.push((lc[0].into(),lc[1].into(),lc[2].into()));
            }
        }
    }
    fs
}



fn main() -> std::io::Result<()> {
    let app = App::new("sol-nft-client")
        .version("0.1.0")
        .about("fetch tokens and addresses of an nft collection")
        .author("dubbelosix")
        .arg(
            Arg::new("creator")
                .short('c')
                .long("creator")
                .help("creator address of the collection. (first in creator array)")
                .required(true)
                .takes_value(true)
            )
        .arg(
            Arg::new("rpc")
                .short('r')
                .long("rpc")
                .help("rpc url to connect to. defaults to genesysgo mainnet")
                .takes_value(true)
                .default_value(RPC_ENDPOINT)
           )
        .arg(
            Arg::new("failed")
                .short('f')
                .long("failed")
                .help("reprocess failed entries from a file")
            )
        .arg(
            Arg::new("threads")
                .short('t')
                .long("threads")
                .takes_value(true)
                .help("set explicit number of threads")
            );


    let matches = app.get_matches();

    let mut creator_address = String::new();
    let mut process_failed = false;
    let mut threads: usize = 20;

    if let Some(i) = matches.value_of("creator"){
        creator_address = i.to_string();
    }
    if matches.is_present("failed") {
        process_failed = true;
    }
    if let Some(i) = matches.value_of("threads"){
        threads = i.parse().expect("threads must be an integer");
    }

    let rpc_endpoint = matches.value_of("rpc").unwrap();
    println!("{},{},{},{}",&creator_address,rpc_endpoint,process_failed,threads);

    rayon::ThreadPoolBuilder::new().num_threads(threads).build_global().unwrap();
    let client = RpcClient::new_with_timeout(String::from(rpc_endpoint),
                                        Duration::from_secs(RPC_TIMEOUT));
    
    let mut fs = FileState {succeeded:Vec::<(String,String,String)>::new(), failed:Vec::<String>::new()};
    let mint_list = if process_failed {
        fs = get_incomplete_mints(&creator_address);
        println!("Processing failed mints - {}",fs.failed.len());
        fs.failed
    } else {
        println!("Getting mints");
        get_list_of_mints_in_collection(&client, &creator_address)
    };

    if mint_list.len() == 0 {
        println!("No mints for creator address: {}", &creator_address);
        return Ok(());
    }
    println!("Getting associated token account list");
    let pb = ProgressBar::new(mint_list.len() as u64);
    let token_account_list : Vec<String> = mint_list.par_iter().progress_with(pb).map(|x| get_token_account_for_mint(x.into())).collect();

    println!("Getting associated token account owners");
    let pb = ProgressBar::new(mint_list.len() as u64);
    let token_account_owner_list: Vec<String> = token_account_list.par_iter().progress_with(pb).map(|x| get_owner_of_assoc_token(&client, x.into())).collect();

    println!("Formatting final results...");
    let mut row_vec: Vec<(String,String,String)> = izip!(mint_list,token_account_owner_list,token_account_list).map(|(x,y,z)| (x,y,z)).collect(); 
    if process_failed {
        for i in fs.succeeded {
            row_vec.push(i);
        }
    }
    let fname = write_mint_info_to_file(creator_address.into(),row_vec).unwrap();
    println!("results in file: {}",fname);

    Ok(())
}
