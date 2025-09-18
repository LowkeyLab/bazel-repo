use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const DEFAULT_SERVER_ID: &str = "89467777677468954757";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Name::Table)
                    .add_column(
                        ColumnDef::new(Name::ServerId)
                            .string()
                            .not_null()
                            .default(DEFAULT_SERVER_ID),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Name::Table)
                    .drop_column(Name::ServerId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Name {
    Table,
    ServerId,
}
