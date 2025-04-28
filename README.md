# Semioscan

**Semioscan** is a Rust tool for analyzing blockchain data, focused on calculating average token prices by looking at on-chain swap events. It’s built to be flexible across different blockchain networks and simple to integrate into broader systems.

## Core Functionality

- Tracks token liquidations by filtering blockchain events
- Calculates average token prices over a specified block range
- Supports multiple chains using a `chain_id` parameter

## Technical Implementation

- Developed in Rust, with Axum handling the API endpoints
- Uses Alloy primitives for interacting with blockchain data
- Supports multiple chains through dynamic provider creation

## API Design

- REST endpoints include: `/api/v1/price` (legacy), `/api/v1/price/v2`, and `/api/v1/price/lo`
- Accepts query parameters like `chain_id`, `token_address`, `from_block`, and `to_block`
- Returns calculated average prices based on swap event data

## Deployment

- Packaged as a Docker container for easy deployment
- Designed to run as a microservice within a blockchain infrastructure
