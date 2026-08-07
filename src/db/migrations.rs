use sqlx::{PgPool, migrate::Migrator};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn apply_schema_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}
