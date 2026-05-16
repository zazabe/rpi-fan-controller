SHELL := /bin/bash

.PHONY: help release-tag

help:
	@echo "Targets:"
	@echo "  make release-tag VERSION=X.Y.Z  - bump Cargo.toml, commit, and tag"

release-tag:
	@if [[ -z "$(VERSION)" ]]; then \
		echo "Usage: make release-tag VERSION=X.Y.Z"; \
		exit 1; \
	fi
	@if ! [[ "$(VERSION)" =~ ^[0-9]+\.[0-9]+\.[0-9]+$$ ]]; then \
		echo "VERSION must match semantic version X.Y.Z (example: 0.1.2)"; \
		exit 1; \
	fi
	@if ! git diff --quiet || ! git diff --cached --quiet; then \
		echo "Working tree is not clean. Commit or stash changes first."; \
		exit 1; \
	fi
	@python3 scripts/bump_cargo_version.py "$(VERSION)"
	@cargo test
	@git add Cargo.toml
	@git commit -m "release: bump version to v$(VERSION)"
	@git tag "v$(VERSION)"
	@echo "Created commit and tag v$(VERSION)."
	@echo "Next step: git push && git push --tags"
