# Database library — software architecture

## Overview

`lgtm-db` is a stateless dummy hello-data package; SQL schema, repositories, migrations and persistence are NA/WIP.

## Requirement

NA/WIP.

### Functional

- FR1: Supply fixed hello data to the backend review service.
- FR2: Leave existing user database files untouched.

### Non-Functional

- NFR1: Open no database connections or files.
- NFR2: Keep a small public boundary for later persistence work.

## Design

NA/WIP.

### Usage

```rust
let database = lgtm_db::Database;
assert_eq!(database.get_hello(), "Hello");
```

#### Connect, insert and query

NA/WIP.

#### Look up a context

NA/WIP.

#### Change records atomically

NA/WIP.

#### Cancel or nest work

NA/WIP.

#### Handle startup and shutdown

The facade has no connection lifecycle; startup does not apply migrations or read `LGTM_DATABASE_URL`.

### Components and Responsibilities

The `Database` facade supplies fixed data; backend services own orchestration and domain models own validation/conversion.

### Data and Control Flow

ReviewService calls `Database::get_hello`, then creates the Review response; no persistence operation takes place.

### Interfaces and Contracts

`Database::get_hello(&self) -> String` returns `Hello` without side effects.

#### Schema and relationships

NA/WIP.

#### Repository queries

NA/WIP.

#### Repository mutations

NA/WIP.

#### Public values and errors

The facade returns a string and has no SQL error or transaction types.

## Implementation

NA/WIP.

### Repositories define the database API

NA/WIP.

### SeaORM supports local SQLite and remote PostgreSQL

NA/WIP.

### Migration implementation

NA/WIP.

### Drivers and verification

SQL drivers and their test matrix are removed; a public package integration test covers the dummy read.

### Review ownership and recovery

NA/WIP.

## References

- [Backend architecture](../backend/index.md).
- [Review contract](../backend/reviews.md).
