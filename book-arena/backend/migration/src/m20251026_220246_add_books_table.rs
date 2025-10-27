use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Users::Name).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Books::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Books::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Books::OpenLibraryWorkId).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserToBook::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserToBook::UserId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_to_book-user_id")
                            .from(UserToBook::Table, UserToBook::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(UserToBook::BookId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_to_book-book_id")
                            .from(UserToBook::Table, UserToBook::BookId)
                            .to(Books::Table, Books::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserToBook::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Books::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Name,
}

#[derive(DeriveIden)]
enum UserToBook {
    Table,
    UserId,
    BookId,
}

#[derive(DeriveIden)]
enum Books {
    Table,
    Id,
    OpenLibraryWorkId,
}
