# sol_nft_client
* fetches all mints from a creator address (first address in the creator array - usually the candy machine address
* rayon with 20 threads (configurable) but keep in mind that an rpc node can ban you for spamming
* rayon (par_iter) to fetch all assoc token addresses
* rayon (par_iter) to fetch owners of assoc token addresses
* uses indicatif, which composes well with rayon for a progress bar
* uses expoential backoffs (backoff) with a max time of 3 minutes
* -f flag with a creator address to retry failed fetches

```
dub@192 % ./target/release/sol_nft_client -h
sol-nft-client 0.1.0
dubbelosix
fetch tokens and addresses of an nft collection

USAGE:
    sol_nft_client [OPTIONS] --creator <creator>

OPTIONS:
    -c, --creator <creator>    creator address of the collection. (first in creator array)
    -f, --failed               reprocess failed entries from a file
    -h, --help                 Print help information
    -r, --rpc <rpc>            rpc url to connect to. defaults to genesysgo mainnet [default:
                               https://ssc-dao.genesysgo.net/]
    -t, --threads <threads>    set explicit number of threads
    -V, --version              Print version information
```

```
dub@192 % ./target/release/sol_nft_client -c "2kiDZiizSdctkQu1Jv5z46AnNxMVH3Fk56afj3urmQBt"
2kiDZiizSdctkQu1Jv5z46AnNxMVH3Fk56afj3urmQBt,https://ssc-dao.genesysgo.net/,false,20
Getting mints
Getting associated token account list
Getting associated token account owners
Formatting final results...
results in file: 2kiDZiizSdctkQu1Jv5z46AnNxMVH3Fk56afj3urmQBt.csv
```    

