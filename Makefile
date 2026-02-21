x`.PHONY: help dev build check clean install test tauri-dev tauri-build frontend-dev frontend-build rust-check

# Default target
.DEFAULT_GOAL := help

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

install: ## Install dependencies (npm + cargo)
	npm install
	cd src-tauri && cargo fetch

dev: ## Run Tauri app in development mode with hot reload
	npm run tauri dev

frontend-dev: ## Run frontend only in development mode
	npm run dev

frontend-build: ## Build frontend (TypeScript check + Vite bundle)
	npm run build

rust-check: ## Check Rust code compilation
	cd src-tauri && cargo check

check: frontend-build rust-check ## Run all checks (frontend + backend)

build: ## Build the full Tauri application
	npm run tauri build

tauri-dev: dev ## Alias for 'make dev'

tauri-build: build ## Alias for 'make build'

clean: ## Clean build artifacts
	rm -rf dist
	rm -rf src-tauri/target
	rm -rf node_modules

test: ## Run tests (placeholder - add your test commands)
	@echo "No tests configured yet"

format: ## Format code (Rust)
	cd src-tauri && cargo fmt

lint: ## Lint Rust code
	cd src-tauri && cargo clippy -- -D warnings

.SILENT: help
