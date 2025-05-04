# Replay

[![ci](https://github.com/tsirysndr/replay/actions/workflows/ci.yml/badge.svg)](https://github.com/tsirysndr/replay/actions/workflows/ci.yml)
[![downloads](https://img.shields.io/crates/dr/replay)](https://crates.io/crates/replay)
[![crates](https://img.shields.io/crates/v/replay.svg)](https://crates.io/crates/replay)

This tool acts as a transparent HTTP proxy that intercepts and records all incoming and outgoing requests and responses. You can later replay these captured interactions to mock the real API without needing live network access — ideal for:

- End-to-end tests
- CI environments
- Offline development
- Contract testing

![Preview](https://raw.githubusercontent.com/tsirysndr/replay/main/.github/assets/preview.png)

## ✨ Features
- 🧲 Record HTTP traffic in real time
- 🧪 Replay and mock previously recorded requests
- 🛠️ Supports REST, GraphQL, and any HTTP-based API
- 📦 Store interactions locally
- ⚡ Fast and lightweight proxy implementation

## 🔧 Example Use Case
1. Run your app through the proxy once to record real API interactions.
2. Save the recorded sessions.
3. Switch to mock mode for testing — no real API calls needed.