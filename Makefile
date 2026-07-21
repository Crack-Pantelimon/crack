# Makefile for GTA Vice City: Pantelimon (Umbre Storyline)

.PHONY: generate status context critique verify clean test

generate:
	python3 scripts/harness.py generate

status:
	python3 scripts/harness.py status

context:
	@if [ -z "$(MISSION)" ]; then \
		echo "Usage: make context MISSION=<num>"; \
		exit 1; \
	fi
	python3 scripts/harness.py context $(MISSION)

critique:
	@if [ -z "$(MISSION)" ]; then \
		echo "Usage: make critique MISSION=<num>"; \
		exit 1; \
	fi
	python3 scripts/harness.py critique $(MISSION)

verify:
	python3 scripts/harness.py verify

test: verify

clean:
	rm -rf docs/missions/*.md
	rm -rf docs/characters/*.md
	rm -f docs/state_machine.md
