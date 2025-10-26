use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Book::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Books::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
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
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(BookToOpenLibraryWorkId::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BookToOpenLibraryWorkId::BookId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BookToOpenLibraryWorkId::OpenLibraryWorkId)
                            .string()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserToBook::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserToBook::UserId).integer().not_null())
                    .col(ColumnDef::new(UserToBook::BookId).integer().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create(
                Index::create()
                    .name("idx-user_to_book-user_id")
                    .table(UserToBook::Table)
                    .col(UserToBook::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create(
                Index::create()
                    .name("idx-user_to_book-book_id")
                    .table(UserToBook::Table)
                    .col(UserToBook::BookId)
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
