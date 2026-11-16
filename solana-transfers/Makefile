START ?= 385870151
STOP  ?= 385870157   # exclusive, so this includes block 385870156
ENDPOINT ?= solana-mainnet.streamingfast.io:443

.PHONY: build run flat

build:
	substreams protogen ./substreams.yaml --exclude-paths="sf/substreams,google"
	cargo build --target wasm32-unknown-unknown --release
	substreams pack ./substreams.yaml

# raw output (one JSON per block)
run:
	substreams run -e $(ENDPOINT) solana-transfers-v0.1.0.spkg map_transfers \
	  -s $(START) -t $(STOP) -o jsonl

# flat output: one {"from","to","amount"} JSON per transfer
flat:
	substreams run -e $(ENDPOINT) solana-transfers-v0.1.0.spkg map_transfers \
	  -s $(START) -t $(STOP) -o jsonl \
	| jq -c '.["@data"].transfers[]? | {from, to, amount: (.amount|tonumber)}'
