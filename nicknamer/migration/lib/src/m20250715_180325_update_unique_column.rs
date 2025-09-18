use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // First, drop the existing unique constraint on discord_id using raw SQL
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE name DROP CONSTRAINT user_discord_id_key;")
            .await?;

        // Modify the column to ensure correct type
        manager
            .alter_table(
                Table::alter()
                    .table(Name::Table)
                    .modify_column(ColumnDef::new(Name::DiscordId).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        // Create a new unique constraint on DiscordId + ServerId
        manager
            .create_index(
                Index::create()
                    .name("name_discord_id_server_id_unique")
                    .table(Name::Table)
                    .col(Name::DiscordId)
                    .col(Name::ServerId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the composite unique constraint
        manager
            .drop_index(
                Index::drop()
                    .name("name_discord_id_server_id_unique")
                    .table(Name::Table)
                    .to_owned(),
            )
            .await?;

        // Recreate the original unique constraint on DiscordId
        manager
            .create_index(
                Index::create()
                    .name("user_discord_id_key")
                    .table(Name::Table)
                    .col(Name::DiscordId)
                    .unique()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Name {
    Table,
    DiscordId,
    ServerId,
}
