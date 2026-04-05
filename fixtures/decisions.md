# Engineering Decisions Log

This document records key technical and architectural decisions made by the team. Each decision includes context, options considered, and the chosen approach.

---

## Decision: Authentication Strategy

**Date**: 2024-10-15
**Status**: Implemented

### Context

We need user authentication for the API gateway. Options evaluated:
1. JWT with symmetric signing (HS256)
2. JWT with asymmetric signing (RS256)
3. Session tokens in PostgreSQL

### Decision

We decided to use JWT with RS256 (asymmetric signing). This allows microservices to verify tokens without sharing a secret, reducing the blast radius of a key compromise.

**Private key** is stored in AWS Secrets Manager. **Public keys** are fetched at startup and cached with a 1-hour TTL.

### Consequences

- Key rotation requires a rolling restart of all services (acceptable for now).
- Token expiry is 15 minutes; refresh tokens are 7 days (stored in httpOnly cookies).

---

## Decision: Database Migration Tool

**Date**: 2024-10-22
**Status**: Implemented

We decided to use `sqlx-cli` for database migrations. Migrations are stored in `migrations/` and run automatically on service startup in staging. Production migrations require a manual approval step via the deployment pipeline.

---

## Decision: API Versioning

**Date**: 2024-11-01
**Status**: In progress

We agreed to use URL-path versioning (`/v1/`, `/v2/`) rather than header-based versioning. This makes versioning explicit in logs and easier to route at the load balancer level.

The current API is `/v1/`. A `/v2/` endpoint will be introduced when breaking changes are needed.

---

## Decision: Error Response Format

**Date**: 2024-11-05
**Status**: Implemented

All API errors follow the RFC 7807 Problem Details format:

```json
{
  "type": "https://errors.example.com/not-found",
  "title": "Resource Not Found",
  "status": 404,
  "detail": "The requested tenant with id '123' was not found.",
  "instance": "/v1/tenants/123"
}
```

---

## Decision: Logging Format

**Date**: 2024-11-10
**Status**: Implemented

We decided to use structured JSON logging (via `tracing` + `tracing-subscriber` in Rust). All log entries include:

- `timestamp` (ISO 8601)
- `level` (trace/debug/info/warn/error)
- `service` (service name)
- `trace_id` (from OpenTelemetry context)
- `message`

This allows correlation across services in Grafana Loki.

---

## Decision: Rate Limiting

**Date**: 2024-11-20
**Status**: Planned

We agreed to implement rate limiting at the API gateway level using a token bucket algorithm. Limits:
- Authenticated users: 1,000 requests/minute
- Unauthenticated: 60 requests/minute
- ML inference endpoint: 10 requests/minute per user

Implementation will use Redis for distributed state.
