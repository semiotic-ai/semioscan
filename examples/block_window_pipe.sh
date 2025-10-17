#!/bin/bash
# Base Router Liquidation Analysis Script
#
# This script:
# 1. Queries the block range for a specific date on Base
# 2. Analyzes USDC transfers for all three Odos routers
# 3. Outputs combined data as JSON for LLM-based report generation
#
# Usage:
#   RPC_URL=https://base-mainnet.g.alchemy.com/v2/ API_KEY=your_api_key ./block_window_pipe.sh

set -e

# Configuration
DATE="2025-10-11"
CHAIN_ID="8453"
TOKEN_ADDRESS="0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"  # USDC on Base

# Router configurations
# V2 Router
V2_ROUTER="0x19ceead7105607cd444f5ad10dd51356436095a1"
V2_RECIPIENT="0xa7471690db0c93a7F827D1894c78Df7379be11c0"

# V3 Router
V3_ROUTER="0x0d05a7d3448512b78fa8a9e46c4872c88c4a0d05"
V3_RECIPIENT="0x498292dc123f19bdbc109081f6cf1d0e849a9daf"

# LO (Limit Order) Router
LO_ROUTER="0xedeafdef0901ef74ee28c207be8424d3b353d97a"
LO_RECIPIENT="0x498292dc123f19bdbc109081f6cf1d0e849a9daf"

echo "Base Router Liquidation Analysis for ${DATE}"
echo "=============================================="
echo ""
echo "Querying block range for ${DATE}..."

# Get block range using block-window command
# Disable logging with RUST_LOG=off to get clean output
BLOCKS=$(RUST_LOG=off cargo run --package semioscan block-window --chain-id ${CHAIN_ID} --date ${DATE} --format plain)

# Parse the block numbers
FROM_BLOCK=$(echo $BLOCKS | cut -d' ' -f1)
TO_BLOCK=$(echo $BLOCKS | cut -d' ' -f2)

echo "Block range: ${FROM_BLOCK} to ${TO_BLOCK}"
echo ""

# Function to run combined analysis and get JSON output
run_analysis() {
    local router_name=$1
    local from_addr=$2
    local to_addr=$3

    echo "Analyzing ${router_name}..." >&2

    # Run the combined query with JSON output, suppress logging
    RUST_LOG=off cargo run --package semioscan combined \
        --chain-id ${CHAIN_ID} \
        --from ${from_addr} \
        --to ${to_addr} \
        --token ${TOKEN_ADDRESS} \
        --from-block ${FROM_BLOCK} \
        --to-block ${TO_BLOCK} \
        --format json
}

# Create JSON output with all three router results
echo ""
echo "Collecting data from all routers..."
echo ""

# Get results for each router
V2_RESULT=$(run_analysis "V2 Router" "$V2_ROUTER" "$V2_RECIPIENT")
V3_RESULT=$(run_analysis "V3 Router" "$V3_ROUTER" "$V3_RECIPIENT")
LO_RESULT=$(run_analysis "LO Router" "$LO_ROUTER" "$LO_RECIPIENT")

# Output combined JSON
echo ""
echo "=============================================="
echo "Combined Analysis Results (JSON)"
echo "=============================================="
echo ""

cat <<EOF
{
  "analysis_date": "${DATE}",
  "chain_id": ${CHAIN_ID},
  "token": "${TOKEN_ADDRESS}",
  "block_range": {
    "from_block": ${FROM_BLOCK},
    "to_block": ${TO_BLOCK}
  },
  "routers": {
    "v2_router": {
      "name": "V2 Router",
      "router_address": "${V2_ROUTER}",
      "recipient_address": "${V2_RECIPIENT}",
      "data": ${V2_RESULT}
    },
    "v3_router": {
      "name": "V3 Router",
      "router_address": "${V3_ROUTER}",
      "recipient_address": "${V3_RECIPIENT}",
      "data": ${V3_RESULT}
    },
    "lo_router": {
      "name": "LO Router",
      "router_address": "${LO_ROUTER}",
      "recipient_address": "${LO_RECIPIENT}",
      "data": ${LO_RESULT}
    }
  }
}
EOF

echo ""
echo "=============================================="
echo "Analysis complete. Data ready for LLM processing."
