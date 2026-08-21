.PHONY: web helper run

web:
	cd web && npm install && npm run build

helper:
	cd helper && cargo build --release

run: web
	cd helper && cargo run --release
