# Block Window CLI

The `block-window` subcommand calculates the block range for a specific UTC date on a blockchain.

## Usage

### Basic Usage (Human-Readable Output)

```bash
RPC_URL=https://arb1.arbitrum.io/rpc/ \
API_KEY=your_api_key \
cargo run --package semioscan block-window --date 2025-10-15
```

Output:
```
Date: 2025-10-15
Block range: [36848527, 36891726] (inclusive)
Block count: 43200
UTC start: 1728950400 (2025-10-15 00:00:00 UTC)
UTC end (exclusive): 1729036800 (2025-10-16 00:00:00 UTC)
```

### JSON Output

```bash
RPC_URL=https://arb1.arbitrum.io/rpc/ \
API_KEY=your_api_key \
cargo run --package semioscan block-window --date 2025-10-15 --format json
```

Output:
```json
{
  "start_block": 36848527,
  "end_block": 36891726,
  "start_ts": {
    "0": 1728950400
  },
  "end_ts_exclusive": {
    "0": 1729036800
  }
}
```

### Plain Output (for Piping)

```bash
RPC_URL=https://arb1.arbitrum.io/rpc/ \
API_KEY=your_api_key \
cargo run --package semioscan block-window --date 2025-10-15 --format plain
```

Output:
```
36848527 36891726
```

## Piping to Other Commands

The `--format plain` output is designed to be easily parsed and piped into other semioscan commands.

**Important**: Disable logging with `RUST_LOG=off` to get clean output for parsing:

```bash
# Get block range for a date (disable logging for clean output)
BLOCKS=$(RUST_LOG=off RPC_URL=https://base-mainnet.g.alchemy.com/v2/ API_KEY=your_api_key \
  cargo run --package semioscan block-window --date 2025-10-15 --format plain)

# Parse the block numbers
FROM_BLOCK=$(echo $BLOCKS | cut -d' ' -f1)
TO_BLOCK=$(echo $BLOCKS | cut -d' ' -f2)

# Use in combined query with JSON output
RUST_LOG=off RPC_URL=https://base-mainnet.g.alchemy.com/v2/ API_KEY=your_api_key \
  cargo run --package semioscan combined \
  --chain-id 8453 \
  --from 0x0D05a7D3448512B78fa8A9e46c4872C88C4a0D05 \
  --to 0x498292DC123f19Bdbc109081f6CF1D0E849A9daF \
  --token 0x833589fcd6edb6e08f4c7c32d4f71b54bda02913 \
  --from-block $FROM_BLOCK \
  --to-block $TO_BLOCK \
  --format json
```

### JSON Output for Programmatic Use

All semioscan commands support `--format json` for structured output:

```bash
# Get combined data as JSON
RUST_LOG=off cargo run --package semioscan combined \
  --chain-id 8453 \
  --from 0x19ceead7105607cd444f5ad10dd51356436095a1 \
  --to 0xa7471690db0c93a7F827D1894c78Df7379be11c0 \
  --token 0x833589fcd6edb6e08f4c7c32d4f71b54bda02913 \
  --from-block 21800000 \
  --to-block 21900000 \
  --format json

# Process with jq
RUST_LOG=off cargo run --package semioscan combined ... --format json | jq '.transaction_count'
```

## Caching

Block window calculations are cached by default in `block_windows.json`. You can specify a custom cache file:

```bash
cargo run --package semioscan block-window \
  --date 2025-10-15 \
  --cache-path my_custom_cache.json
```

The cache stores results by chain ID and date, so repeated queries for the same date are instantaneous.

## Options

- `--date <YYYY-MM-DD>`: Required. The UTC date to query (format: YYYY-MM-DD)
- `--cache-path <PATH>`: Optional. Path to cache file (default: `block_windows.json`)
- `--format <FORMAT>`: Optional. Output format: `human` (default), `json`, or `plain`

## Environment Variables

- `RPC_URL`: Required. The RPC endpoint URL for the blockchain
- `API_KEY`: Required. API key for the RPC endpoint

The command combines these as `{RPC_URL}{API_KEY}/` to support Pinax-style endpoints.

## Examples

### Arbitrum (Chain ID 42161)

```bash
RPC_URL=https://arb1.arbitrum.io/rpc/ \
API_KEY=your_api_key \
cargo run --package semioscan block-window --date 2025-10-15
```

### Base (Chain ID 8453)

```bash
RPC_URL=https://base-mainnet.g.alchemy.com/v2/ \
API_KEY=your_api_key \
cargo run --package semioscan block-window --date 2025-10-15
```

### Ethereum (Chain ID 1)

```bash
RPC_URL=https://eth-mainnet.g.alchemy.com/v2/ \
API_KEY=your_api_key \
cargo run --package semioscan block-window --date 2025-10-15
```

## How It Works

The block window calculator uses binary search to efficiently find:
1. The first block with timestamp >= 00:00:00 UTC on the given date
2. The last block with timestamp <= 23:59:59 UTC on the given date

This approach is more efficient than scanning all blocks and provides exact boundaries for daily analysis.

Results are cached to avoid repeated RPC calls when querying the same date multiple times.
