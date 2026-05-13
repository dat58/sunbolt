SHELL := /bin/bash

ifneq (,$(wildcard .env))
include .env
export
endif

CONTROL_BIND_ADDR ?= 127.0.0.1:3000
CONTROL_PLANE_URL ?= http://127.0.0.1:3000
UI_PORT ?= 8080
UI_ORIGIN ?= http://127.0.0.1:$(UI_PORT)
ALLOWED_ORIGINS ?= *
ADMIN_EMAIL ?= $(if $(SUNBOLT_DEV_ADMIN_EMAIL),$(SUNBOLT_DEV_ADMIN_EMAIL),admin@sunbolt.local)
ADMIN_PASSWORD ?= $(if $(SUNBOLT_DEV_ADMIN_PASSWORD),$(SUNBOLT_DEV_ADMIN_PASSWORD),sunbolt-dev-admin)
AGENT_NODE_NAME ?= local-agent
ENROLLMENT_TTL_SECS ?= 900
PRODUCTION_ENV_FILE ?= config/production.env
IMAGE ?= sunbolt:latest
CONTAINER_NAME ?= sunbolt-control
HOST_PORT ?= 3000
CONTAINER_PORT ?= 3000
HEALTH_URL ?= http://127.0.0.1:$(HOST_PORT)/health
BACKUP_DIR ?= backups
MIGRATION_DIR ?= migrations
DOCKER ?= docker
PSQL ?= psql
PG_DUMP ?= pg_dump
SEA_ORM_CLI ?= sea-orm-cli

.PHONY: help env-init prod-env-init prod-env-check control ui-css ui-css-watch ui agent-token agent fmt-check checks release-checks docker-build db-check db-backup db-migrate docker-run docker-restart health deploy

help:
	@printf '%s\n' \
		'Sunbolt local targets:' \
		'  make env-init      # copy .env.example to .env if needed' \
		'  make control       # run the control plane on $(CONTROL_BIND_ADDR)' \
		'  make ui-css        # build the Dioxus UI CSS bundle once' \
		'  make ui-css-watch  # watch and rebuild the UI CSS bundle' \
		'  make ui            # serve the Dioxus UI on port $(UI_PORT)' \
		'  make agent-token   # print a fresh one-time agent enrollment token' \
		'  make agent         # enroll and run a local agent node' \
		'  make checks        # cargo test + cargo clippy' \
		'' \
		'Sunbolt production deploy targets:' \
		'  make prod-env-init # copy config/production.env.example to $(PRODUCTION_ENV_FILE)' \
		'  make release-checks # cargo fmt --check + cargo test + cargo clippy' \
		'  make docker-build  # build $(IMAGE)' \
		'  make db-check      # verify production PostgreSQL connectivity' \
		'  make db-backup     # write a PostgreSQL backup under $(BACKUP_DIR)' \
		'  make db-migrate    # run SeaORM migrations against production DB' \
		'  make docker-run    # start $(CONTAINER_NAME) from $(IMAGE)' \
		'  make docker-restart # replace $(CONTAINER_NAME) with $(IMAGE)' \
		'  make health        # check $(HEALTH_URL)' \
		'  make deploy        # release-checks + build + DB backup/migrate + restart + health'

env-init:
	@if [[ -f .env ]]; then \
		echo '.env already exists'; \
	else \
		cp .env.example .env; \
		echo 'created .env from .env.example'; \
	fi

prod-env-init:
	@if [[ -f "$(PRODUCTION_ENV_FILE)" ]]; then \
		echo '$(PRODUCTION_ENV_FILE) already exists'; \
	else \
		cp config/production.env.example "$(PRODUCTION_ENV_FILE)"; \
		chmod 600 "$(PRODUCTION_ENV_FILE)"; \
		echo 'created $(PRODUCTION_ENV_FILE) from config/production.env.example'; \
	fi

prod-env-check:
	@set -euo pipefail; \
	if [[ ! -f "$(PRODUCTION_ENV_FILE)" ]]; then \
		echo "$(PRODUCTION_ENV_FILE) is missing; run 'make prod-env-init' and edit it" >&2; \
		exit 1; \
	fi; \
	set -a; source "$(PRODUCTION_ENV_FILE)"; set +a; \
	if [[ "$${SUNBOLT_ENV:-}" != "production" ]]; then \
		echo 'SUNBOLT_ENV must be production' >&2; \
		exit 1; \
	fi; \
	if [[ -z "$${SUNBOLT_DATABASE_URL:-}" ]]; then \
		echo 'SUNBOLT_DATABASE_URL is required' >&2; \
		exit 1; \
	fi; \
	if [[ "$${SUNBOLT_PUBLIC_URL:-}" != https://* ]]; then \
		echo 'SUNBOLT_PUBLIC_URL must be an HTTPS URL' >&2; \
		exit 1; \
	fi; \
	if [[ -z "$${SUNBOLT_ALLOWED_ORIGINS:-}" || "$${SUNBOLT_ALLOWED_ORIGINS}" == '*' ]]; then \
		echo 'SUNBOLT_ALLOWED_ORIGINS must list explicit HTTPS origins' >&2; \
		exit 1; \
	fi; \
	if [[ "$${SUNBOLT_COOKIE_SECURE:-}" != "true" ]]; then \
		echo 'SUNBOLT_COOKIE_SECURE must be true' >&2; \
		exit 1; \
	fi; \
	if [[ "$${SUNBOLT_DEV_BOOTSTRAP_ADMIN:-}" == "true" ]]; then \
		echo 'SUNBOLT_DEV_BOOTSTRAP_ADMIN must not be true in production' >&2; \
		exit 1; \
	fi; \
	if grep -Eq 'replace-me|example\.com' "$(PRODUCTION_ENV_FILE)"; then \
		echo "$(PRODUCTION_ENV_FILE) still contains example values" >&2; \
		exit 1; \
	fi; \
	echo 'production environment file passed local deploy checks'

control:
	SUNBOLT_BIND_ADDR=$(CONTROL_BIND_ADDR) \
	SUNBOLT_ALLOWED_ORIGINS='$(ALLOWED_ORIGINS)' \
	cargo run -p sunbolt-control

ui-css:
	cd crates/sunbolt-ui && npm run css:build

ui-css-watch:
	cd crates/sunbolt-ui && npm run css:watch

ui: ui-css
	SUNBOLT_CONTROL_PLANE_URL=$(CONTROL_PLANE_URL) \
	dx serve --platform web --package sunbolt-ui --port $(UI_PORT)

agent-token:
	@set -euo pipefail; \
	COOKIE_JAR="$$(mktemp)"; \
	TOKEN_RESPONSE="$$(mktemp)"; \
	trap 'rm -f "$$COOKIE_JAR" "$$TOKEN_RESPONSE"' EXIT; \
	if ! curl -fsS $(CONTROL_PLANE_URL)/health >/dev/null; then \
		echo "control plane is not reachable at $(CONTROL_PLANE_URL); start it with 'make control'" >&2; \
		exit 1; \
	fi; \
	if ! curl -fsS \
		-c "$$COOKIE_JAR" \
		-H 'content-type: application/json' \
		-X POST $(CONTROL_PLANE_URL)/auth/login \
		-d '{"email":"$(ADMIN_EMAIL)","password":"$(ADMIN_PASSWORD)"}' >/dev/null; then \
		echo "admin login failed for $(ADMIN_EMAIL); check .env or Make variables" >&2; \
		exit 1; \
	fi; \
	if ! curl -fsS \
		-b "$$COOKIE_JAR" \
		-H 'content-type: application/json' \
		-X POST $(CONTROL_PLANE_URL)/nodes/enrollment-tokens \
		-d '{"expires_in_secs":$(ENROLLMENT_TTL_SECS)}' \
		-o "$$TOKEN_RESPONSE"; then \
		echo "failed to create an enrollment token from $(CONTROL_PLANE_URL)" >&2; \
		exit 1; \
	fi; \
	python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["token"])' "$$TOKEN_RESPONSE"

agent:
	@TOKEN="$$( $(MAKE) --no-print-directory agent-token )"; \
	echo "starting agent with one-time enrollment token"; \
	SUNBOLT_CONTROL_PLANE_URL=$(CONTROL_PLANE_URL) \
	SUNBOLT_AGENT_NODE_NAME='$(AGENT_NODE_NAME)' \
	SUNBOLT_AGENT_ENROLLMENT_TOKEN="$$TOKEN" \
	cargo run -p sunbolt-agent

fmt-check:
	cargo fmt --all -- --check

checks:
	cargo test
	cargo clippy --all-targets --all-features -- -D warnings

release-checks:
	$(MAKE) --no-print-directory fmt-check
	$(MAKE) --no-print-directory checks

docker-build:
	$(DOCKER) build -t $(IMAGE) .

db-check: prod-env-check
	@set -euo pipefail; \
	set -a; source "$(PRODUCTION_ENV_FILE)"; set +a; \
	$(PSQL) "$$SUNBOLT_DATABASE_URL" -c "select 1"

db-backup: prod-env-check
	@set -euo pipefail; \
	set -a; source "$(PRODUCTION_ENV_FILE)"; set +a; \
	mkdir -p "$(BACKUP_DIR)"; \
	BACKUP_FILE="$(BACKUP_DIR)/sunbolt-$$(date +%Y%m%d%H%M%S).dump"; \
	$(PG_DUMP) --format=custom --file="$$BACKUP_FILE" "$$SUNBOLT_DATABASE_URL"; \
	echo "created $$BACKUP_FILE"

db-migrate: prod-env-check
	@set -euo pipefail; \
	set -a; source "$(PRODUCTION_ENV_FILE)"; set +a; \
	$(SEA_ORM_CLI) migrate up -d "$(MIGRATION_DIR)" -u "$$SUNBOLT_DATABASE_URL"

docker-run: prod-env-check
	$(DOCKER) run -d \
		--name $(CONTAINER_NAME) \
		--env-file $(PRODUCTION_ENV_FILE) \
		-p $(HOST_PORT):$(CONTAINER_PORT) \
		--restart unless-stopped \
		$(IMAGE)

docker-restart: prod-env-check
	@$(DOCKER) rm -f $(CONTAINER_NAME) >/dev/null 2>&1 || true
	$(MAKE) --no-print-directory docker-run

health:
	curl -fsS "$(HEALTH_URL)"

deploy:
	$(MAKE) --no-print-directory prod-env-check
	$(MAKE) --no-print-directory release-checks
	$(MAKE) --no-print-directory docker-build
	$(MAKE) --no-print-directory db-check
	$(MAKE) --no-print-directory db-backup
	$(MAKE) --no-print-directory db-migrate
	$(MAKE) --no-print-directory docker-restart
	$(MAKE) --no-print-directory health
