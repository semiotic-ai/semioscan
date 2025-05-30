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

## Calculate Gas

```bash
RUST_LOG=info cargo run --bin semioscan -- gas --chain-id 137 --from 0x4E3288c9ca110bCC82bf38F09A7b425c095d92Bf --to 0x498020622CA0d5De103b7E78E3eFe5819D0d28AB --token 0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359 --from-block 71559546 --to-block 71559546
```

You should be able to see the [details of that transaction on polygonscan](https://polygonscan.com/tx/0xa8eccf2546db7d440e0639734053dbbcc68b7928852f63ad6815ea5cf7bbea3d).

## Calculate Liqudation Amount

```bash
RUST_LOG=info cargo run --bin semioscan -- transfer-amount --chain-id 42161 --router 0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13 --to 0x3c440a8653d6bad527a96d0f8bff55a934a2a67f --token 0xaf88d065e77c8cC2239327C5EDb3A432268e5831 --from-block 306126306 --to-block 315667779 
```

This will calculate the liquidated amount in USDC delivered to the recipient between the two blocks inclusive.
