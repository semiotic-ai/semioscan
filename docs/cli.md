# CLI (DEPRECATED)

> **⚠️ DEPRECATION NOTICE**
>
> As of semioscan v0.2.0, this crate is a **library-only** package. The CLI and API server binaries described in this document have been removed.
>
> **For current usage**, see the [README](../README.md) and the [examples/](../examples/) directory which demonstrate how to use semioscan as a library in your own applications.
>
> This document is preserved for historical reference only. For v0.1.x CLI documentation, see the [v0.1.x branch](https://github.com/semiotic-ai/likwid/tree/v0.1.x/crates/semioscan).

---

## API Server (REMOVED IN v0.2.0)

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

Calculations for gas costs that should include L1 data fees will be done on OP Stack chains like Base and Optimism:

```bash
RUST_LOG=info cargo run --bin semioscan -- gas --chain-id 8453 --from 0x0000000000000000000000000000000000000000 --to 0xa7471690db0c93a7F827D1894c78Df7379be11c0 --token 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 --from-block 30991400 --to-block 30991400
```

Details for that transaction on Base can be found [on BaseScan](https://basescan.org/tx/0x534b7efe2cb97417fdbbd7c7cff9f69c85504cec88d9d6ae0b7b2db92ff87607).

## Calculate Liqudation Amount

```bash
RUST_LOG=info cargo run --bin semioscan -- transfer-amount --chain-id 42161 --router 0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13 --to 0x3c440a8653d6bad527a96d0f8bff55a934a2a67f --token 0xaf88d065e77c8cC2239327C5EDb3A432268e5831 --from-block 306126306 --to-block 315667779 
```

This will calculate the liquidated amount in USDC delivered to the recipient between the two blocks inclusive.
