from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest
import uuid


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "migrate_sqlite_to_postgres",
    ROOT / "scripts/migrate-sqlite-to-postgres.py",
)
assert SPEC is not None and SPEC.loader is not None
MIGRATION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MIGRATION)


class HistoricalSqliteCompatibilityTests(unittest.TestCase):
    def setUp(self) -> None:
        self.connection = sqlite3.connect(":memory:")

    def tearDown(self) -> None:
        self.connection.close()

    def test_missing_historical_columns_use_explicit_defaults(self) -> None:
        columns = next(columns for table, columns in MIGRATION.TABLES if table == "review_states")
        self.connection.execute(
            """
            CREATE TABLE review_states (
                id INTEGER, card_id INTEGER, algorithm_used TEXT, state_data TEXT,
                repeats INTEGER, status TEXT, daily_refreshed_at TEXT, next_review_at TEXT
            )
            """
        )
        self.connection.execute(
            "INSERT INTO review_states VALUES (1, 2, 'sm2', '{}', 3, 'active', NULL, '2026-01-01')"
        )

        block, count = MIGRATION.copy_block(
            self.connection, "review_states", columns, {"review_states"}
        )

        self.assertEqual(count, 1)
        self.assertIn("sm2", block)
        self.assertIn("active", block)
        self.assertIn("\\N", block)

    def test_new_feature_table_may_be_absent(self) -> None:
        columns = next(columns for table, columns in MIGRATION.TABLES if table == "image_pool")

        block, count = MIGRATION.copy_block(self.connection, "image_pool", columns, set())

        self.assertEqual(block, "")
        self.assertEqual(count, 0)

    def test_unrecognized_missing_column_is_rejected(self) -> None:
        self.connection.execute("CREATE TABLE users (id TEXT)")
        columns = next(columns for table, columns in MIGRATION.TABLES if table == "users")

        with self.assertRaisesRegex(SystemExit, "missing required columns"):
            MIGRATION.copy_block(self.connection, "users", columns, {"users"})

    @unittest.skipUnless(os.environ.get("DATABASE_URL"), "DATABASE_URL is required")
    def test_historical_database_imports_once_into_postgres(self) -> None:
        database_url = os.environ["DATABASE_URL"]
        schema = f"migration_test_{uuid.uuid4().hex}"
        with tempfile.TemporaryDirectory() as directory:
            sqlite_path = Path(directory) / "denpie.db"
            self._create_historical_fixture(sqlite_path)
            command = [
                sys.executable,
                str(ROOT / "scripts/migrate-sqlite-to-postgres.py"),
                "--sqlite",
                str(sqlite_path),
                "--database-url",
                database_url,
                "--schema",
                schema,
            ]
            try:
                first = subprocess.run(command, capture_output=True, text=True, check=False)
                self.assertEqual(first.returncode, 0, first.stderr)
                verification = subprocess.run(
                    MIGRATION.psql_command(database_url)
                    + [
                        "-At",
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-c",
                        (
                            f'SET search_path TO "{schema}", public; '
                            "SELECT (SELECT COUNT(*) FROM users) || '|' || "
                            "(SELECT COUNT(*) FROM tipcards) || '|' || "
                            "(SELECT llm_vision_model = llm_model FROM user_settings) || '|' || "
                            "(SELECT feedback = '' AND reviewed_at IS NULL FROM review_states);"
                        ),
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(verification.returncode, 0, verification.stderr)
                self.assertEqual(verification.stdout.strip().splitlines()[-1], "1|1|true|true")

                second = subprocess.run(command, capture_output=True, text=True, check=False)
                self.assertNotEqual(second.returncode, 0)
                self.assertIn("target Denpie schema is not empty", second.stderr)
            finally:
                subprocess.run(
                    MIGRATION.psql_command(database_url)
                    + [
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-c",
                        f'DROP SCHEMA IF EXISTS "{schema}" CASCADE;',
                    ],
                    capture_output=True,
                    check=False,
                )

    @staticmethod
    def _create_historical_fixture(path: Path) -> None:
        connection = sqlite3.connect(path)
        connection.executescript(
            """
            CREATE TABLE users (
                id TEXT, username TEXT, password_hash TEXT, role TEXT,
                created_at TEXT, display_name TEXT, avatar_data TEXT
            );
            CREATE TABLE api_keys (
                id INTEGER, key_hash TEXT, client_name TEXT, created_at TEXT, user_id TEXT
            );
            CREATE TABLE topics (
                id INTEGER, name TEXT, class_id INTEGER, prompt_template TEXT,
                daily_card_count INTEGER, daily_time_zone TEXT, daily_update_time TEXT,
                compression_level TEXT, tipcard_type TEXT, user_id TEXT,
                icon_id TEXT, color_hue INTEGER
            );
            CREATE TABLE tipcards (
                id INTEGER, topic_id INTEGER, tipcard_type TEXT, title TEXT,
                full_content TEXT, compressed_content TEXT, image_data TEXT,
                pinned INTEGER, created_at TEXT, user_id TEXT
            );
            CREATE TABLE review_states (
                id INTEGER, card_id INTEGER, algorithm_used TEXT, state_data TEXT,
                status TEXT, daily_refreshed_at TEXT, next_review_at TEXT, repeats INTEGER
            );
            CREATE TABLE tipcard_images (
                id INTEGER, user_id TEXT, card_id INTEGER, position INTEGER,
                storage_path TEXT, mime_type TEXT, byte_size INTEGER, created_at TEXT
            );
            CREATE TABLE llm_token_usage (
                id INTEGER, model TEXT, purpose TEXT, prompt_tokens INTEGER,
                completion_tokens INTEGER, total_tokens INTEGER, created_at TEXT, user_id TEXT
            );
            CREATE TABLE user_settings (
                user_id TEXT, llm_model TEXT, llm_compress_model TEXT, prompt_template TEXT,
                llm_api_key TEXT, llm_base_url TEXT, llm_compress_base_url TEXT,
                llm_reasoning_effort TEXT, llm_compress_reasoning_effort TEXT,
                llm_compression_level TEXT, color_scheme TEXT, transparency INTEGER,
                blur_intensity INTEGER, daily_time_zone TEXT, daily_update_time TEXT,
                max_active_cards INTEGER
            );
            CREATE TABLE daily_refresh_runs (
                user_id TEXT, topic_id INTEGER, tipcard_type TEXT,
                window_start TEXT, refreshed_at TEXT
            );
            CREATE TABLE passkeys (passkey_id BLOB, user_id TEXT, passkey TEXT);

            INSERT INTO users VALUES (
                'user-1', 'admin', 'hash', 'admin', '2026-01-01T00:00:00Z', 'Admin', NULL
            );
            INSERT INTO topics VALUES (
                1, 'Topic', 1, NULL, 1, 'UTC', '09:00', 'short',
                'repeatable_tip', 'user-1', NULL, 42
            );
            INSERT INTO tipcards VALUES (
                1, 1, 'repeatable_tip', 'Card', 'Full', 'Short', '[]', 0,
                '2026-01-01T00:00:00Z', 'user-1'
            );
            INSERT INTO review_states VALUES (
                1, 1, 'sm2', '{}', 'active', NULL, '2026-01-02T00:00:00Z', 0
            );
            INSERT INTO llm_token_usage VALUES (
                1, 'model', 'test', 1, 2, 3, '2026-01-01T00:00:00Z', 'user-1'
            );
            INSERT INTO user_settings VALUES (
                'user-1', 'model', 'compress-model', 'prompt', 'key',
                'https://llm.invalid', 'https://compress.invalid', 'low', 'low',
                'short', 'dark', 0, 0, 'UTC', '09:00', 20
            );
            INSERT INTO daily_refresh_runs VALUES (
                'user-1', 1, 'repeatable_tip', '2026-01-01T00:00:00Z',
                '2026-01-01T00:00:00Z'
            );
            INSERT INTO passkeys VALUES (X'010203', 'user-1', '{}');
            """
        )
        connection.commit()
        connection.close()


if __name__ == "__main__":
    unittest.main()
