# CLI

## API Server

```bash
cargo run --bin semioscan api -- --port 3000
```

Then, in the browser, navigate to 

`http://localhost:3000/api/v1/price/v2?chain_id=8453&token_address=0x78a087d713Be963Bf307b18F2Ff8122EF9A63ae9&from_block=29490100&to_block=29499100`

where: 

- `8453` is the chain id for Base, 
- `0x78a087d713Be963Bf307b18F2Ff8122EF9A63ae9` is the address of the token, 
- `29490100` is the starting block for the price calculation, and 
- `29499100` is the ending block for the price calculation.

## Calculate Price

```bash
cargo run --bin semioscan price -- --chain-id 8453 --token-address 0x78a087d713Be963Bf307b18F2Ff8122EF9A63ae9 --from-block 29490100 --to-block 29499100 --router-type v2
```

This will calculate the average price of the token between the two blocks for the v2 signer.

```bash
cargo run --bin semioscan price -- --chain-id 8453 --token-address 0x78a087d713Be963Bf307b18F2Ff8122EF9A63ae9 --from-block 29490100 --to-block 29499100 --router-type lo
```

This will calculate the average price of the token between the two blocks for the limit
order signer, i.e. for all non-v2 routers on that chain.
