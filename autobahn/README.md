# Autobahn WebSocket Test Suite

The [Autobahn TestSuite](https://github.com/crossbario/autobahn-testsuite) is the industry-standard compliance test for WebSocket implementations.

## Prerequisites

- Docker installed and running
- rsws built with default features

## Running the Tests

### 1. Start the test server

```bash
cargo run --example autobahn_server
```

### 2. Run Autobahn test suite

In a separate terminal:

```bash
docker run -it --rm \
  -v "${PWD}/autobahn:/config" \
  -v "${PWD}/autobahn/reports:/reports" \
  --network host \
  crossbario/autobahn-testsuite \
  wstest -m fuzzingclient -s /config/fuzzingclient.json
```

### 3. View results

Open `autobahn/reports/server/index.html` in a browser.

## Test Categories

The Autobahn suite tests:

| Category | Description |
|----------|-------------|
| 1.x | Framing |
| 2.x | Pings/Pongs |
| 3.x | Reserved bits |
| 4.x | Opcodes |
| 5.x | Fragmentation |
| 6.x | UTF-8 handling |
| 7.x | Close handling |
| 9.x | Limits/performance |
| 10.x | Auto-fragmentation |
| 12.x-13.x | WebSocket compression (permessage-deflate) |

## Interpreting Results

- **Pass (green)**: Test passed
- **Non-strict (yellow)**: Passed with minor deviations
- **Fail (red)**: Test failed

## Configuration

Edit `fuzzingclient.json` to customize:

```json
{
  "cases": ["1.*", "2.*"],     // Run specific categories
  "exclude-cases": ["9.*"]     // Skip performance tests
}
```
