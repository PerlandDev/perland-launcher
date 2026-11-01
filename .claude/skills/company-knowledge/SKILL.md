---
name: company-knowledge
description: Use before answering or deciding anything about Perland's production infrastructure, hosts, servers, hosting, capacity, costs, pricing, deployments, Coolify, DNS, backups, team decisions, incidents or ownership, in PerlandEngine, PerlandPanel, PerlandDev-site or perland-launcher. Reads Dosu (the team's knowledge base) first, so answers rest on current facts rather than dated specs.
---

# Company knowledge

The repositories say how the code works. They often do not say what is true about the company now: which machines production runs on, what they cost, who decided what, what broke last week. That lives in Dosu, the team's knowledge base, connected to these sessions as an MCP server (`read_knowledge`, `write_knowledge`, `finalize_session_knowledge`; the tool name's prefix differs per session).

Why this exists: a session once answered a pricing question with "production runs on one Hetzner CCX23 (4 vCPU, 16 GB)", taken from a dated design spec. Production had moved to a dedicated server weeks before and the Hetzner box was deleted. Dosu and the runbook said so; the session only looked after the owner asked.

## Before answering

1. **Read Dosu first.** Call `read_knowledge` with a detailed query (what you need and why). Do this before reading repository docs on the topic, and before stating a fact about production, hosts, costs, deployments, decisions, incidents or ownership.
2. **Then the runbooks.** In PerlandEngine, `docs/runbooks/` (`production-hosts.md` for the hosts, DNS and Coolify). Other repositories: their own runbooks or deployment docs, if any.
3. **Treat dated docs as history.** Design specs (`docs/superpowers/specs/<date>-...`), plans and older guides describe the world on their date. Use them for why something was built, never as the current state of production.
4. **When sources disagree, say so.** Name both sources and their dates, prefer the newer one, and tell the person which you used. Do not quietly pick one.
5. **When nothing settles it, say that too.** Do not fill the gap from memory or from a spec.

## After the work

- **Write back what is durable.** When you learned something non-obvious that the next session would otherwise rediscover (a host fact, a decision and its reason, an incident's cause, a gotcha), save it with `write_knowledge`. Not task summaries, progress, test results, guesses, secrets or personal data.
- **Finalize once.** When `read_knowledge` or `write_knowledge` returned a `receipt_item_id` this turn, call `finalize_session_knowledge` exactly once at the end of the turn, with every receipt, right before your final reply.

## When not to

Plain code questions the code answers (how a handler maps an error, what a test expects) need no Dosu read. If unsure whether company context matters, read first: it is one call.
