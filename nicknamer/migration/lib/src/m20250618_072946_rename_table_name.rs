use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .rename_table(Table::rename().table(User::Table, Name::Table).to_owned())
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .rename_table(Table::rename().table(Name::Table, User::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum User {
    Table,
}

#[derive(Iden)]
enum Name {
    Table,
}
