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

.PHONY: help env-init control ui-css ui-css-watch ui agent-token agent checks

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
		'  make checks        # cargo test + cargo clippy'

env-init:
	@if [[ -f .env ]]; then \
		echo '.env already exists'; \
	else \
		cp .env.example .env; \
		echo 'created .env from .env.example'; \
	fi

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

checks:
	cargo test
	cargo clippy --all-targets --all-features -- -D warnings
