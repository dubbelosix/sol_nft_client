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
    core::time::Duration,
    clap::{Parser},
    bs58,
    ureq,

    rayon::prelude::*,
};

const RPC_ENDPOINT: &str = "https://ssc-dao.genesysgo.net/";
const METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
const METADATA_CREATOR_ADDRESS_0_OFFSET: usize = 326;

// setting this high because fetching tokens from the same creator_0 (candymachine) looks like it
// scans a lot of accounts on the rpc server side. It scans accounts matching a pattern in the
// metadata and can cause a timeout with default config. there has to be a better way to get this
// information (or for the rpc server to provide it). seems like it'll get worse as more token
// metadata accounts are created

const RPC_TIMEOUT: u64 = 300;

fn get_address_filter_for_program(address: String, offset: usize) -> RpcProgramAccountsConfig {
    let memcmp = RpcFilterType::Memcmp(Memcmp{ 
        offset: offset,
        bytes: MemcmpEncodedBytes::Base58(address),
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

fn get_token_account_for_mint(mint_addr_str: String)-> Option<String> {
    let request_body = GetTokenLargestAccounts {
        jsonrpc: "2.0".into(),
        id:1,
        method:"getTokenLargestAccounts".into(),
        params: vec![mint_addr_str]
    };
    let resp = ureq::post(RPC_ENDPOINT).send_json(request_body).unwrap();
    let token_response: GetTokenLargestAccountsResponse = resp.into_json().unwrap(); 
    for holder in token_response.result.value {
        if holder.uiAmount == 1.0 {
            return Some(holder.address.into())
        }
    }
    None
}

fn get_owner_of_assoc_token(client: &RpcClient, token_account: String) -> String {
    let token_account_data: UiTokenAccount = client.get_token_account_with_commitment(&Pubkey::new(&bs58::decode(token_account).into_vec().unwrap())
                                   ,CommitmentConfig{commitment:CommitmentLevel::Finalized}).unwrap().value.unwrap();
    token_account_data.owner
}

fn get_list_of_mints_in_collection(client: &RpcClient, creator0_address: String) -> Vec<String> {
    let metadata_program_pubkey: Pubkey = Pubkey::new(&bs58::decode(METADATA_PROGRAM).into_vec().unwrap());
    let result_vec = client.get_program_accounts_with_config(&metadata_program_pubkey,
                                                             get_address_filter_for_program(
                                                                 creator0_address,
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


fn main() {

    let client = RpcClient::new_with_timeout(String::from(RPC_ENDPOINT),
                                        Duration::from_secs(RPC_TIMEOUT));

    let mint_list = get_list_of_mints_in_collection(&client, "DVemJ8n9ZiSmSf8a18VYgpBgTUoHEA8x6ZZBsTL2bxk9".into());

    for i in mint_list {
        println!("{}",i);
    }
    println!("=======");

    let token_account = get_token_account_for_mint(String::from("5xRe9LuuHQUh1EUhMfizWRu7cC972UReife9SzSiy4QV")).unwrap();
    println!("{}",token_account);

    let token_account_owner = get_owner_of_assoc_token(&client, token_account);
    println!("{}", token_account_owner);
    
}
