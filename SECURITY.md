# Security Policy

## Supported Versions

We release patches for security vulnerabilities for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.3.x   | :white_check_mark: |
| 0.2.x   | :white_check_mark: |
| < 0.2.0 | :x:                |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

If you discover a security vulnerability in semioscan, please report it by emailing:

**<joseph@semiotic.ai>**

Please include:

1. **Description** of the vulnerability
2. **Steps to reproduce** the issue
3. **Potential impact** of the vulnerability
4. **Suggested fix** (if you have one)
5. **Your contact information** for follow-up questions

We will acknowledge your email within 48 hours and provide a more detailed response within 7 days indicating the next steps in handling your report.

After the initial reply to your report, the security team will endeavor to keep you informed of the progress being made towards a fix and full announcement. We may ask for additional information or guidance surrounding the reported issue.

## Security Considerations for Semioscan

### RPC Trust Boundary

**Semioscan trusts the RPC provider completely.** The library does not verify:

- Block hashes or signatures
- Transaction signatures
- Chain reorganizations
- Data authenticity

**Implications:**

- Always use trusted RPC providers (Alchemy, Infura, QuickNode, or your own node)
- For production systems, consider running your own node for maximum security
- Be aware that a malicious or compromised RPC provider could:
  - Return incorrect gas costs
  - Provide false price data
  - Manipulate block timestamps and numbers

### Input Validation

Semioscan validates inputs to prevent common vulnerabilities:

- **Integer overflow protection**: Uses saturating arithmetic for gas calculations
- **Block range validation**: Checks for valid block ranges before querying RPC
- **Address validation**: Uses Alloy's type-safe `Address` type

### Cryptographic Operations

Semioscan does NOT perform cryptographic operations or key management:

- No private key handling
- No transaction signing
- No signature verification
- No encryption/decryption

This library is for **read-only blockchain analytics** and does not handle sensitive cryptographic material.

### Dependencies

We strive to minimize dependencies and keep them up-to-date:

- Regular dependency audits using `cargo audit`
- Automated dependency updates via Dependabot
- Only use well-maintained, audited dependencies (Alloy, tokio, serde, etc.)

To check for known vulnerabilities in dependencies:

```bash
cargo install cargo-audit
cargo audit
```

### Memory Safety

Semioscan is written in Rust and benefits from its memory safety guarantees:

- No unsafe code in the library (verified with `grep -r "unsafe" src`)
- No panics in library code (all errors use `Result<T, E>`)
- File system operations use advisory locks (not enforced by OS)

### Denial of Service (DoS) Considerations

Potential DoS vectors and mitigations:

1. **Large block ranges**:
   - **Risk**: Querying millions of blocks could exhaust memory or time out
   - **Mitigation**: `MaxBlockRange` configuration limits query size
   - **User responsibility**: Set appropriate `max_block_range` for your RPC provider

2. **Rate limiting**:
   - **Risk**: Rapid RPC requests could trigger rate limiting or IP bans
   - **Mitigation**: Configurable `rate_limit_delay` per chain
   - **User responsibility**: Configure appropriate delays for your RPC tier

3. **Timeout handling**:
   - **Risk**: Unresponsive RPC providers could hang indefinitely
   - **Mitigation**: Configurable `rpc_timeout` (default 30 seconds)
   - **User responsibility**: Set appropriate timeouts for your use case

4. **Cache file size**:
   - **Risk**: Malicious cache files could cause OOM
   - **Mitigation**: Currently none (advisory locks only)
   - **Recommendation**: Store cache files in trusted locations only

### Production Deployment Recommendations

For production deployments:

1. **Use dedicated RPC endpoints**
   - Don't share RPC keys across services
   - Use rate-limited or premium RPC tiers
   - Monitor RPC usage and costs

2. **Implement circuit breakers**
   - Detect and handle prolonged RPC failures
   - Implement exponential backoff for retries
   - Alert on sustained failures

3. **Monitor and log**
   - Log all RPC errors for debugging
   - Track gas cost calculation failures
   - Monitor price data quality

4. **Validate critical data**
   - Cross-reference gas costs across multiple blocks
   - Sanity-check price data against known values
   - Alert on anomalies

5. **Secure cache storage**
   - Use read-only file systems when possible
   - Restrict cache file permissions
   - Validate cache data integrity

## Disclosure Policy

When a security vulnerability is reported:

1. **Confirmation**: We confirm the vulnerability and determine its severity
2. **Fix Development**: We develop a fix in a private repository
3. **Notification**: We notify major users (if applicable) before public disclosure
4. **Release**: We release a patched version
5. **Announcement**: We publish a security advisory on GitHub
6. **Credit**: We credit the reporter in the security advisory (unless they prefer to remain anonymous)

**Timeline**: We aim to release security patches within 30 days of a confirmed vulnerability.

## Security Audit Status

**Semioscan has not undergone a formal security audit.**

The library has been:

- Battle-tested in production processing millions of dollars in DeFi swaps
- Reviewed by staff engineers at Semiotic AI
- Analyzed with `cargo clippy` (zero warnings)
- Tested with 313 unit and property-based tests

For critical production deployments, consider commissioning your own security audit.

## Contact

For general security inquiries: <joseph@semiotic.ai>
For vulnerability reports: <joseph@semiotic.ai> (private, will not be published)

---

**Last Updated**: 2025-11-16
