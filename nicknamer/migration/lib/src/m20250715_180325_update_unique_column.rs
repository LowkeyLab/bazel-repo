use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove the old unique index
        manager
            .drop_index(
                Index::drop()
                    .name("idx-user-user_id_from_discord-server_id")
                    .to_owned(),
            )
            .await?;

        // Create a new unique index on just user_id_from_discord
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-user-user_id_from_discord")
                    .table(User::Table)
                    .col(User::UserIdFromDiscord)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove the new unique index
        manager
            .drop_index(
                Index::drop()
                    .name("idx-user-user_id_from_discord")
                    .to_owned(),
            )
            .await?;

        // Recreate the old unique index
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-user-user_id_from_discord-server_id")
                    .table(User::Table)
                    .col(User::UserIdFromDiscord)
                    .col(User::ServerId)
                    .unique()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum User {
    Table,
    UserIdFromDiscord,
    ServerId,
}
