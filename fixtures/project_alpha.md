# Project Alpha

## Overview

Project Alpha is our internal initiative to build a distributed data pipeline for real-time analytics. The project uses a microservices architecture deployed on Kubernetes, with Rust services for data ingestion and Python for ML model serving.

## Tech Stack

- **Ingestion layer**: Rust (tokio + axum)
- **Message queue**: Apache Kafka with 3 brokers
- **Storage**: PostgreSQL 15 for structured data, S3-compatible object store for raw files
- **ML serving**: Python (FastAPI + PyTorch)
- **Orchestration**: Kubernetes (k8s) with Helm charts
- **Observability**: Prometheus + Grafana dashboards

## Architecture Decisions

### Decision: Use Rust for the ingestion layer

We decided to use Rust for the ingestion layer after benchmarking showed a 4x throughput improvement over the Python prototype. The memory safety guarantees also reduce operational risk. The team agreed to onboard two engineers to Rust over the next sprint.

### Decision: Single-region deployment for MVP

Agreed to start with us-east-1 only. Multi-region failover will be addressed in Phase 2. This keeps infrastructure costs under $3,000/month for the MVP.

### Decision: Kafka over RabbitMQ

We evaluated RabbitMQ and Kafka. Kafka was chosen for its log retention semantics, which allow replay of events during ML model retraining. RabbitMQ does not support this natively.

## Team

- **Tech Lead**: Alice Chen (alice@example.com)
- **Backend Engineers**: Bob Kumar, Carol Zhang
- **ML Engineer**: David Park
- **DevOps**: Erin Torres

## Current Status (as of 2024-Q4)

The ingestion service is handling 50,000 events/second in staging. We are targeting 100,000 events/second for production go-live in Q1 2025.

Known issues:
1. Memory spike during Kafka consumer rebalancing — being investigated by Bob.
2. PostgreSQL write amplification at high throughput — considering partitioning by tenant_id.
3. Grafana dashboard latency exceeds 2s — possibly a cardinality issue in Prometheus labels.

## Glossary

- **Event**: A single data point produced by a sensor or user action, serialized as JSON.
- **Ingestion lag**: Time between event production and storage confirmation. Target: <500ms P95.
- **Tenant**: A paying customer whose data is logically isolated via tenant_id partitioning.
- **Dead letter queue (DLQ)**: Kafka topic where malformed or unprocessable events are sent.
