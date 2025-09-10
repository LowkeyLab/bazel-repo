pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_user_table;
mod m20250618_072946_rename_table_name;
mod m20250622_231317_add_index;
mod m20250706_102217_add_name_by_server;
mod m20250715_180325_update_unique_column;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_user_table::Migration),
            Box::new(m20250618_072946_rename_table_name::Migration),
            Box::new(m20250622_231317_add_index::Migration),
            Box::new(m20250706_102217_add_name_by_server::Migration),
            Box::new(m20250715_180325_update_unique_column::Migration),
        ]
    }
}
