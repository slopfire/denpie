#!/usr/bin/env python3
"""Copy a Denpie SQLite database into an empty PostgreSQL schema."""

from __future__ import annotations

import argparse
import csv
import io
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys


TABLES: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("users", ("id", "username", "password_hash", "role", "display_name", "avatar_data", "created_at")),
    ("api_keys", ("id", "user_id", "key_hash", "client_name", "created_at")),
    (
        "topics",
        (
            "id", "user_id", "name", "tipcard_type", "prompt_template", "daily_card_count",
            "daily_time_zone", "daily_update_time", "compression_level", "icon_id", "color_hue",
            "grounding_strategy", "image_strategy",
        ),
    ),
    (
        "tipcards",
        (
            "id", "user_id", "topic_id", "tipcard_type", "title", "full_content",
            "compressed_content", "use_image", "image_query", "image_data", "pinned", "created_at",
        ),
    ),
    (
        "review_states",
        (
            "id", "card_id", "algorithm_used", "state_data", "repeats", "status", "feedback",
            "reviewed_at", "daily_refreshed_at", "next_review_at",
        ),
    ),
    (
        "tipcard_images",
        ("id", "user_id", "card_id", "position", "storage_path", "mime_type", "byte_size", "created_at"),
    ),
    (
        "llm_token_usage",
        (
            "id", "user_id", "model", "purpose", "prompt_tokens", "completion_tokens",
            "total_tokens", "created_at",
        ),
    ),
    (
        "user_settings",
        (
            "user_id", "llm_model", "llm_grounding_model", "llm_vision_model", "llm_compress_model",
            "prompt_template", "llm_api_key", "llm_base_url", "llm_compress_base_url",
            "llm_reasoning_effort", "llm_grounding_reasoning_effort", "llm_compress_reasoning_effort",
            "llm_compression_level", "daily_time_zone", "daily_update_time", "max_active_cards",
            "grounding_strategy", "image_strategy", "search_provider", "scrape_provider",
            "search_api_key", "search_base_url", "image_sources",
        ),
    ),
    (
        "daily_refresh_runs",
        ("user_id", "topic_id", "tipcard_type", "window_start", "refreshed_at"),
    ),
    ("passkeys", ("passkey_id", "user_id", "passkey")),
    (
        "user_documents",
        ("id", "user_id", "source_type", "title", "url", "content", "created_at"),
    ),
    ("document_topics", ("document_id", "topic_id")),
    (
        "image_pool",
        (
            "id", "user_id", "storage_path", "mime_type", "byte_size", "name", "tags",
            "description", "created_at",
        ),
    ),
    ("document_chunks", ("document_id", "user_id", "chunk")),
)

SEQUENCED_TABLES = (
    "api_keys", "topics", "tipcards", "review_states", "tipcard_images",
    "llm_token_usage", "user_documents", "image_pool",
)

# The production SQLite database predates several PostgreSQL-era fields.  Keep
# the compatibility policy explicit: a missing column is accepted only when a
# deterministic value can be derived without changing existing data.
MISSING_COLUMN_EXPRESSIONS: dict[tuple[str, str], str] = {
    ("topics", "grounding_strategy"): "NULL",
    ("topics", "image_strategy"): "NULL",
    ("tipcards", "use_image"): "0",
    ("tipcards", "image_query"): "''",
    ("review_states", "feedback"): "''",
    ("review_states", "reviewed_at"): "NULL",
    ("user_settings", "llm_grounding_model"): '"llm_model"',
    ("user_settings", "llm_vision_model"): '"llm_model"',
    ("user_settings", "llm_grounding_reasoning_effort"): "''",
    ("user_settings", "grounding_strategy"): "'factual'",
    ("user_settings", "image_strategy"): "'none'",
    ("user_settings", "search_provider"): "'tavily'",
    ("user_settings", "scrape_provider"): "'scrapling'",
    ("user_settings", "search_api_key"): "''",
    ("user_settings", "search_base_url"): "'https://api.tavily.com'",
    ("user_settings", "image_sources"): "'[]'",
}

# These feature tables were added after the oldest supported SQLite schema.
# Their absence means that the feature had no rows to migrate.
OPTIONAL_EMPTY_TABLES = {
    "user_documents",
    "document_topics",
    "image_pool",
    "document_chunks",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sqlite", default="denpie.db", type=Path, help="source SQLite file")
    parser.add_argument(
        "--database-url",
        default=os.environ.get("DATABASE_URL", "postgres://denpie:denpie@127.0.0.1:5432/denpie"),
        help="target PostgreSQL URL (defaults to DATABASE_URL)",
    )
    parser.add_argument("--schema", default="public", help="empty target PostgreSQL schema")
    return parser.parse_args()


def psql_command(database_url: str) -> list[str]:
    if shutil.which("psql"):
        return ["psql", database_url]
    local_compose_urls = {
        "postgres://denpie:denpie@127.0.0.1:5432/denpie",
        "postgres://denpie:denpie@localhost:5432/denpie",
    }
    if (
        database_url in local_compose_urls
        and shutil.which("docker")
        and Path("compose.dev.yaml").is_file()
    ):
        return [
            "docker", "compose", "-f", "compose.dev.yaml", "exec", "-T", "postgres",
            "psql", "-U", "denpie", "-d", "denpie",
        ]
    raise SystemExit(
        "psql is required for this target (or use the bundled local database with `just db-up`)"
    )


def source_tables(connection: sqlite3.Connection) -> set[str]:
    return {
        row[0]
        for row in connection.execute("SELECT name FROM sqlite_master WHERE type IN ('table', 'view')")
    }


def source_columns(connection: sqlite3.Connection, table: str) -> set[str]:
    return {row[1] for row in connection.execute(f'PRAGMA table_info("{table}")')}


def csv_value(value: object) -> object:
    if value is None:
        return r"\N"
    if isinstance(value, bytes):
        return "\\x" + value.hex()
    return value


def copy_block(
    connection: sqlite3.Connection,
    table: str,
    columns: tuple[str, ...],
    present_tables: set[str],
) -> tuple[str, int]:
    if table not in present_tables:
        if table in OPTIONAL_EMPTY_TABLES:
            return "", 0
        raise SystemExit(f"source database is missing required table: {table}")

    available = source_columns(connection, table)
    missing = [
        column
        for column in columns
        if column not in available and (table, column) not in MISSING_COLUMN_EXPRESSIONS
    ]
    if missing:
        raise SystemExit(f"source table {table} is missing required columns: {', '.join(missing)}")

    output = io.StringIO(newline="")
    writer = csv.writer(output, lineterminator="\n")
    selections = [
        f'"{column}"'
        if column in available
        else f'{MISSING_COLUMN_EXPRESSIONS[(table, column)]} AS "{column}"'
        for column in columns
    ]
    query = f'SELECT {", ".join(selections)} FROM "{table}"'
    rows = connection.execute(query)
    count = 0
    for row in rows:
        writer.writerow(csv_value(value) for value in row)
        count += 1
    if count == 0:
        return "", 0

    block = (
        f'COPY "{table}" ({", ".join(f"\"{column}\"" for column in columns)}) '
        "FROM STDIN WITH (FORMAT csv, NULL '\\N');\n"
        + output.getvalue()
        + "\\.\n"
    )
    return block, count


def rows_copy_block(table: str, columns: tuple[str, ...], rows: list[tuple[object, ...]]) -> str:
    if not rows:
        return ""
    output = io.StringIO(newline="")
    writer = csv.writer(output, lineterminator="\n")
    for row in rows:
        writer.writerow(csv_value(value) for value in row)
    return (
        f'COPY "{table}" ({", ".join(f"\"{column}\"" for column in columns)}) '
        "FROM STDIN WITH (FORMAT csv, NULL '\\N');\n"
        + output.getvalue()
        + "\\.\n"
    )


def main() -> int:
    args = parse_args()
    if not args.sqlite.is_file():
        raise SystemExit(f"SQLite source does not exist: {args.sqlite}")
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", args.schema):
        raise SystemExit("--schema must be a PostgreSQL identifier")

    connection = sqlite3.connect(f"file:{args.sqlite.resolve()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    present = source_tables(connection)
    missing_tables = [
        table
        for table, _ in TABLES
        if table not in present and table not in OPTIONAL_EMPTY_TABLES
    ]
    if missing_tables:
        raise SystemExit("source database is missing tables: " + ", ".join(missing_tables))

    owner_tables = [
        table
        for table, columns in TABLES
        if "user_id" in columns and table != "users" and table in present
    ]
    owner_union = " UNION ".join(f'SELECT user_id FROM "{table}"' for table in owner_tables)
    orphan_user_ids = [
        row[0]
        for row in connection.execute(
            f"SELECT user_id FROM ({owner_union}) owners "
            "WHERE user_id IS NOT NULL AND user_id NOT IN (SELECT id FROM users)"
        )
    ]
    topic_references = [
        "SELECT topic_id, user_id, tipcard_type FROM tipcards",
        "SELECT topic_id, user_id, tipcard_type FROM daily_refresh_runs",
    ]
    if "document_topics" in present and "user_documents" in present:
        topic_references.append(
            "SELECT links.topic_id, docs.user_id, 'repeatable_tip' "
            "FROM document_topics links "
            "JOIN user_documents docs ON docs.id = links.document_id"
        )
    orphan_topics = list(
        connection.execute(
            "SELECT refs.topic_id, MIN(refs.user_id), MIN(refs.tipcard_type) "
            f"FROM ({' UNION ALL '.join(topic_references)}) refs "
            "LEFT JOIN topics ON topics.id = refs.topic_id "
            "WHERE topics.id IS NULL "
            "GROUP BY refs.topic_id"
        )
    )

    root = Path(__file__).resolve().parent.parent
    migration = (root / "migrations/0001_schema.sql").read_text()
    counts: dict[str, int] = {}
    copy_blocks: list[str] = []
    for table, columns in TABLES:
        block, count = copy_block(connection, table, columns, present)
        counts[table] = count
        if block:
            copy_blocks.append(block)
        if table == "users" and orphan_user_ids:
            synthetic_users = [
                (
                    user_id,
                    f"migrated-orphan-{index + 1}",
                    None,
                    "user",
                    "Migrated deleted user",
                    None,
                    "1970-01-01T00:00:00Z",
                )
                for index, user_id in enumerate(orphan_user_ids)
            ]
            copy_blocks.append(rows_copy_block(table, columns, synthetic_users))
            counts[table] += len(synthetic_users)
        if table == "topics" and orphan_topics:
            synthetic_topics = [
                (
                    topic_id,
                    user_id,
                    f"Migrated deleted topic {topic_id}",
                    tipcard_type or "repeatable_tip",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                for topic_id, user_id, tipcard_type in orphan_topics
            ]
            copy_blocks.append(rows_copy_block(table, columns, synthetic_topics))
            counts[table] += len(synthetic_topics)
    connection.close()

    qualified_schema = f'"{args.schema}"'
    checks = " + ".join(f"(SELECT COUNT(*) FROM \"{table}\")" for table, _ in TABLES)
    count_assertions = "\n".join(
        f"    IF (SELECT COUNT(*) FROM \"{table}\") != {counts[table]} THEN "
        f"RAISE EXCEPTION 'row-count mismatch for {table}'; END IF;"
        for table, _ in TABLES
    )
    sequences = "\n".join(
        f"SELECT setval(pg_get_serial_sequence('{table}', 'id'), "
        f"COALESCE((SELECT MAX(id) FROM \"{table}\"), 1), "
        f"EXISTS (SELECT 1 FROM \"{table}\"));"
        for table in SEQUENCED_TABLES
    )
    sql = f"""\
\\set ON_ERROR_STOP on
BEGIN;
CREATE SCHEMA IF NOT EXISTS {qualified_schema};
SET search_path TO {qualified_schema}, public;
{migration}
DO $$
BEGIN
    IF ({checks}) > 0 THEN
        RAISE EXCEPTION 'target Denpie schema is not empty';
    END IF;
END $$;
{''.join(copy_blocks)}
{sequences}
DO $$
BEGIN
{count_assertions}
END $$;
COMMIT;
"""

    command = psql_command(args.database_url)
    completed = subprocess.run(command, input=sql.encode(), cwd=root, check=False)
    if completed.returncode != 0:
        return completed.returncode

    print("SQLite to PostgreSQL migration complete:")
    for table, _ in TABLES:
        print(f"  {table}: {counts[table]}")
    if orphan_user_ids:
        print(f"  synthesized placeholder owners: {len(orphan_user_ids)}")
    if orphan_topics:
        print(f"  synthesized placeholder topics: {len(orphan_topics)}")
    print("The source SQLite file was opened read-only and was not modified.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
