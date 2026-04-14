# Persistence Layer

"Any application is only as stable as its ACID-compliant backing store." -Sun Tzu

## Overview

KF2 uses Diesel to map persistence-layer application models to an underlying database table. The current database of choice is sqlite with WAL, since the current architecture doesn't require data sharing between multiple kf2 server processes. Diesel simplifies the generation of SQL and adapting structs to SQL queries, and makes it a little less painful to switch between databases in the future.

## Architecture

The persistence layer is composed of two modules:

- `db` contains the actual database connection logic, connection pooling, etc
- `repo` contains the bulk of the code, including the Rust schema and the CRUD/query layer

In more antiquated terms, `repo` is the Data Access Object (DAO) layer, using the Diesel ORM to map between Rust datatypes and database tables. Each queryable model gets a `...Repo` trait in `repo` and an implementer in  `repo::diesel_impl`. For simple models which are directly used in the application layer, this may just be a simple adapter between the blocking sqlite calls and the async application code. Other models may require more complex mapping logic; either way, they should live in the `repo` module.

## Data Modeling

Ideally, structs stored into the database are separate from the structs used throughout the application logic. This way, it's clear whether changes to application models also require changes to database models, and application logic can be refactored without worry of affecting the database. Diesel provides many derived traits which make working with database models basically painless, which means it is easier to keep structs in their respective domains.

Having said that, there are some structs and logic which truly are best represented as a bag of data stored in the database. A good example of this would be a song queue entry: its existence as an application model is inconsequential beyond its role of being stored in and read from the database. Having a separate struct for a queue entry would just be extra boilerplate that serves no purpose.

The one upside to all of this is that it seems Diesel provides type-checking for SQL queries at compile time. So, if you did end up adding a field to a persisted model without also updating the schema, you'll know when you try to build. Magic!

### SQLite Implementation Notes

- By default, foreign keys are not enabled, custom code enables them manually per-connection.
- WAL journaling needs to be specifically enabled as well.
- Tests can avail themselves of an in-memory database to avoid cluttering the filesystem with sqlite DBs.

## Schema Changes

Inevitably, you will need to change the database schema. The [Diesel Getting Started guide](https://diesel.rs/guides/getting-started/) provides a good overview of the procedures involved.

Migrations live in folders under `src/server/migrations`, and contain an `up.sql` and `down.sql` corresponding to the apply and rollback operations. They're embedded into the built binary and applied automatically on startup.

There are two ways to write migrations, pure-SQL or assisted. You can also vibecode the migration (this is the key). If writing the migration by hand, the easiest (claim is dubious) way is to:

1. Have a database with the full "before" schema (schema can be applied via `DATABASE_URL=kf2.db diesel migration run`).
2. Modify `repo/schema.rs` to reflect the desired new schema; this can be done concurrently with defining the new struct fields.
3. Run `diesel migration generate --diff-schema [migration name]`
