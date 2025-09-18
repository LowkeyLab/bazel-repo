use nicknamer_server::entities::name;
use nicknamer_server::name::NameService;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection, EntityTrait};
use testcontainers_modules::{postgres, testcontainers};

mod common;

// 1. Define TestContext struct locally
pub struct TestContext {
    #[allow(dead_code)] // container is kept to ensure it's not dropped
    pub container: testcontainers::ContainerAsync<postgres::Postgres>,
    pub db: DatabaseConnection,
}

// 2. Define setup() function locally, using public functions from common module
async fn setup() -> anyhow::Result<TestContext> {
    // Allow multiple calls to init for tests.
    let _ = tracing_subscriber::fmt().try_init();
    let container = common::setup_container().await?;
    let db = common::setup_db(&container).await?;
    Ok(TestContext { db, container })
}

#[tokio::test]
async fn can_register_name() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);
    let discord_id = 123456789;
    let name = "TestUser".to_string();
    let created_name = name_service
        .create_name(discord_id, name.clone(), "server123".to_string())
        .await
        .expect("Failed to create name");

    // Verify that the created name has the correct properties
    assert_eq!(created_name.discord_id(), discord_id);
    assert_eq!(created_name.name(), &name);
    assert!(created_name.id() > 0); // ID should be generated and positive
}

#[tokio::test]
async fn can_update_name() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a name entry directly using the entity ActiveModel
    let initial_discord_id = 987654321;
    let initial_name = "InitialName".to_string();
    let active_model = name::ActiveModel {
        discord_id: ActiveValue::Set(initial_discord_id),
        name: ActiveValue::Set(initial_name.clone()),
        server_id: ActiveValue::Set("server123".to_string()),
        ..Default::default()
    };
    let initial_name_entry = active_model
        .insert(&state.db)
        .await
        .expect("Failed to create name");

    let expected_initial_name = nicknamer_server::name::Name::from(initial_name_entry.clone());
    let service_initial_name = nicknamer_server::name::Name::from(initial_name_entry.clone());
    assert_eq!(service_initial_name, expected_initial_name);

    // Edit the name
    let new_name = "UpdatedName".to_string();
    let updated_name = name_service
        .edit_name_by_id(
            initial_name_entry.id as u32,
            new_name.clone(),
            "server456".to_string(),
        )
        .await
        .expect("Failed to update name");

    let expected_updated_name = {
        let mut expected_model = initial_name_entry.clone();
        expected_model.name = new_name.clone();
        expected_model.server_id = "server456".to_string();
        nicknamer_server::name::Name::from(expected_model)
    };
    assert_eq!(updated_name, expected_updated_name);
}

#[tokio::test]
async fn can_handle_update_when_name_not_found() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a name entry directly using the entity ActiveModel to ensure there's some data
    let active_model = name::ActiveModel {
        discord_id: ActiveValue::Set(111222333),
        name: ActiveValue::Set("SomeUser".to_string()),
        server_id: ActiveValue::Set("server789".to_string()),
        ..Default::default()
    };
    let initial_name = active_model
        .insert(&state.db)
        .await
        .expect("Failed to create name");

    // Verify that an error is returned if the name ID does not exist
    let non_existent_id = initial_name.id + 1; // Assuming this ID won't exist
    let result = name_service
        .edit_name_by_id(
            non_existent_id as u32,
            "AnotherName".to_string(),
            "server999".to_string(),
        )
        .await;
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(
            e.to_string(),
            format!("Name entry with ID {} not found", non_existent_id)
        );
    }
}

#[tokio::test]
async fn can_get_all_names() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a couple of name entries directly using the entity ActiveModel
    let name1_discord_id = 1;
    let name1_name = "UserOne".to_string();
    let active_model1 = name::ActiveModel {
        discord_id: ActiveValue::Set(name1_discord_id),
        name: ActiveValue::Set(name1_name.clone()),
        ..Default::default()
    };
    let created_name1 = active_model1
        .insert(&state.db)
        .await
        .expect("Failed to create name1");

    let name2_discord_id = 2;
    let name2_name = "UserTwo".to_string();
    let active_model2 = name::ActiveModel {
        discord_id: ActiveValue::Set(name2_discord_id),
        name: ActiveValue::Set(name2_name.clone()),
        ..Default::default()
    };
    let created_name2 = active_model2
        .insert(&state.db)
        .await
        .expect("Failed to create name2");

    let names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert_eq!(names.len(), 2);

    let expected_name1 = nicknamer_server::name::Name::from(created_name1);
    let expected_name2 = nicknamer_server::name::Name::from(created_name2);

    assert!(names.contains(&expected_name1));
    assert!(names.contains(&expected_name2));
}

#[tokio::test]
async fn can_handle_empty_names_list() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    let names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert!(names.is_empty());
}

#[tokio::test]
async fn cannot_create_name_with_duplicate_discord_id_and_server_id() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);
    let discord_id = 123456789;
    let server_id = "server123";

    // First name creation should succeed
    let first_name = name_service
        .create_name(discord_id, "FirstUser".to_string(), server_id.to_string())
        .await
        .expect("Failed to create first name");

    assert_eq!(first_name.discord_id(), discord_id);
    assert_eq!(first_name.name(), "FirstUser");

    // Second name creation with same Discord ID and same Server ID should fail
    let second_creation_result = name_service
        .create_name(discord_id, "SecondUser".to_string(), server_id.to_string())
        .await;

    assert!(second_creation_result.is_err());
    if let Err(e) = second_creation_result {
        assert!(e.to_string().contains("already exists"));
    }
}

#[tokio::test]
async fn can_create_name_with_same_discord_id_but_different_server_id() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);
    let discord_id = 123456789;

    // First name creation should succeed
    let first_name = name_service
        .create_name(discord_id, "FirstUser".to_string(), "server123".to_string())
        .await
        .expect("Failed to create first name");

    assert_eq!(first_name.discord_id(), discord_id);
    assert_eq!(first_name.name(), "FirstUser");

    // Second name creation with same Discord ID but different Server ID should succeed
    let second_creation_result = name_service
        .create_name(
            discord_id,
            "SecondUser".to_string(),
            "server456".to_string(),
        )
        .await;

    assert!(second_creation_result.is_ok());
    let second_name = second_creation_result.unwrap();
    assert_eq!(second_name.discord_id(), discord_id);
    assert_eq!(second_name.name(), "SecondUser");
    assert_eq!(second_name.server_id(), "server456");
}

#[tokio::test]
async fn can_delete_name() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a name to delete using ActiveModel
    let discord_id: i64 = 123456789;
    let name = "TestUser".to_string();
    let active_model = name::ActiveModel {
        discord_id: ActiveValue::Set(discord_id),
        name: ActiveValue::Set(name.clone()),
        ..Default::default()
    };
    let created_name_model = active_model
        .insert(&state.db)
        .await
        .expect("Failed to create name");
    let created_name = nicknamer_server::name::Name::from(created_name_model);

    // Verify it was created
    let names_before = name::Entity::find()
        .all(&state.db)
        .await
        .expect("Failed to get all names from database");
    assert_eq!(names_before.len(), 1);

    // Delete the name
    let deleted_name = name_service
        .delete_name_by_id(created_name.id())
        .await
        .expect("Failed to delete name");

    // Verify the deleted name matches what was created
    assert_eq!(deleted_name.id(), created_name.id());
    assert_eq!(deleted_name.discord_id(), discord_id as u64);
    assert_eq!(deleted_name.name(), &name);

    // Verify it was deleted
    let names_after = name::Entity::find()
        .all(&state.db)
        .await
        .expect("Failed to get all names from database");
    assert!(names_after.is_empty());
}

#[tokio::test]
async fn can_handle_delete_when_name_not_found() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Try to delete a non-existent name
    let result = name_service.delete_name_by_id(999).await;

    // Should return an error
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("not found") || e.to_string().contains("NameNotFound"));
    }
}

#[tokio::test]
async fn can_get_name_by_id() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a name entry directly using the entity ActiveModel
    let discord_id = 555666777;
    let name = "GetTestUser".to_string();
    let active_model = name::ActiveModel {
        discord_id: ActiveValue::Set(discord_id),
        name: ActiveValue::Set(name.clone()),
        ..Default::default()
    };
    let created_name_model = active_model
        .insert(&state.db)
        .await
        .expect("Failed to create name");

    // Get the name by ID
    let retrieved_name = name_service
        .get_name_by_id(created_name_model.id as u32)
        .await
        .expect("Failed to get name by ID");

    // Construct expected result and compare
    let expected_name = nicknamer_server::name::Name::from(created_name_model);
    assert_eq!(retrieved_name, expected_name);
}

#[tokio::test]
async fn can_handle_get_name_by_nonexistent_id() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a name entry to ensure we have some data and know what ID won't exist
    let active_model = name::ActiveModel {
        discord_id: ActiveValue::Set(777888999),
        name: ActiveValue::Set("ExistingUser".to_string()),
        ..Default::default()
    };
    let created_name = active_model
        .insert(&state.db)
        .await
        .expect("Failed to create name");

    // Try to get a name with a non-existent ID
    let non_existent_id = created_name.id + 100; // Ensure this ID won't exist
    let result = name_service.get_name_by_id(non_existent_id as u32).await;

    // Should return an error
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(
            e.to_string(),
            format!("Name entry with ID {} not found", non_existent_id)
        );
    }
}

#[tokio::test]
async fn can_get_multiple_names_by_different_ids() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create multiple name entries
    let names_data = vec![
        (111222333, "FirstUser".to_string()),
        (444555666, "SecondUser".to_string()),
        (777888999, "ThirdUser".to_string()),
    ];

    let mut created_models = Vec::new();
    for (discord_id, name) in names_data {
        let active_model = name::ActiveModel {
            discord_id: ActiveValue::Set(discord_id),
            name: ActiveValue::Set(name),
            ..Default::default()
        };
        let created_model = active_model
            .insert(&state.db)
            .await
            .expect("Failed to create name");
        created_models.push(created_model);
    }

    // Retrieve each name by ID and verify
    for created_model in created_models {
        let retrieved_name = name_service
            .get_name_by_id(created_model.id as u32)
            .await
            .expect("Failed to get name by ID");

        let expected_name = nicknamer_server::name::Name::from(created_model);
        assert_eq!(retrieved_name, expected_name);
    }
}

#[tokio::test]
async fn can_handle_malformed_yaml_in_bulk_create() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Test with invalid YAML content
    let invalid_yaml = "invalid yaml content: not properly formatted";
    let server_id = "test-server".to_string();

    let result = name_service
        .bulk_create_names(invalid_yaml, server_id)
        .await;

    // Should return MalformedData error
    assert!(result.is_err());
    let error = result.unwrap_err();
    match error {
        nicknamer_server::name::NameServiceError::MalformedData(msg) => {
            assert!(msg.contains("Invalid YAML format"));
        }
        _ => panic!("Expected MalformedData error, got: {:?}", error),
    }
}

#[tokio::test]
async fn can_bulk_create_names_from_valid_yaml() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create valid YAML content with multiple entries
    let yaml_content = r#"
123456789: "Alice"
987654321: "Bob"
555666777: "Charlie"
"#;
    let server_id = "test-server".to_string();

    let result = name_service
        .bulk_create_names(yaml_content, server_id.clone())
        .await
        .expect("Failed to bulk create names");

    let (created_count, skipped_count, errors) = result;
    assert_eq!(created_count, 3);
    assert_eq!(skipped_count, 0);
    assert!(errors.is_empty());

    // Verify all names were created correctly
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert_eq!(all_names.len(), 3);

    // Check that each expected name exists
    let expected_names = vec![
        (123456789u64, "Alice"),
        (987654321u64, "Bob"),
        (555666777u64, "Charlie"),
    ];

    for (expected_discord_id, expected_name) in expected_names {
        let found_name = all_names
            .iter()
            .find(|name| {
                name.discord_id() == expected_discord_id
                    && name.name() == expected_name
                    && name.server_id() == server_id
            })
            .unwrap_or_else(|| {
                panic!(
                    "Expected name with Discord ID {} and name '{}' not found",
                    expected_discord_id, expected_name
                )
            });
        assert!(found_name.id() > 0);
    }
}

#[tokio::test]
async fn can_bulk_create_names_with_empty_yaml() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Test with empty YAML content
    let yaml_content = "{}";
    let server_id = "test-server".to_string();

    let result = name_service
        .bulk_create_names(yaml_content, server_id)
        .await
        .expect("Failed to bulk create names from empty YAML");

    let (created_count, skipped_count, errors) = result;
    assert_eq!(created_count, 0);
    assert_eq!(skipped_count, 0);
    assert!(errors.is_empty());

    // Verify no names were created
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert!(all_names.is_empty());
}

#[tokio::test]
async fn can_bulk_create_names_and_skip_duplicates() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    let server_id = "test-server".to_string();

    // First, create an existing name entry
    name_service
        .create_name(123456789, "ExistingUser".to_string(), server_id.clone())
        .await
        .expect("Failed to create existing name");

    // Now try to bulk create names, including the duplicate
    let yaml_content = r#"
123456789: "Alice"
987654321: "Bob"
555666777: "Charlie"
"#;

    let result = name_service
        .bulk_create_names(yaml_content, server_id.clone())
        .await
        .expect("Failed to bulk create names");

    let (created_count, skipped_count, errors) = result;
    assert_eq!(created_count, 2); // Bob and Charlie created
    assert_eq!(skipped_count, 1); // Alice skipped (duplicate)
    assert!(errors.is_empty());

    // Verify correct names exist
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert_eq!(all_names.len(), 3); // ExistingUser + Bob + Charlie

    // Check that the original name is unchanged
    let existing_name = all_names
        .iter()
        .find(|name| name.discord_id() == 123456789)
        .expect("Expected existing name not found");
    assert_eq!(existing_name.name(), "ExistingUser"); // Original name preserved

    // Check that new names were created
    let expected_new_names = vec![(987654321u64, "Bob"), (555666777u64, "Charlie")];

    for (expected_discord_id, expected_name) in expected_new_names {
        let found_name = all_names
            .iter()
            .find(|name| {
                name.discord_id() == expected_discord_id
                    && name.name() == expected_name
                    && name.server_id() == server_id
            })
            .unwrap_or_else(|| {
                panic!(
                    "Expected name with Discord ID {} and name '{}' not found",
                    expected_discord_id, expected_name
                )
            });
        assert!(found_name.id() > 0);
    }
}

#[tokio::test]
async fn can_bulk_create_names_with_same_discord_id_different_servers() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    let server1_id = "server1".to_string();
    let server2_id = "server2".to_string();

    // Create names in first server
    let yaml_content = r#"
123456789: "Alice"
987654321: "Bob"
"#;

    let result1 = name_service
        .bulk_create_names(yaml_content, server1_id.clone())
        .await
        .expect("Failed to bulk create names for server1");

    let (created_count1, skipped_count1, errors1) = result1;
    assert_eq!(created_count1, 2);
    assert_eq!(skipped_count1, 0);
    assert!(errors1.is_empty());

    // Create names with same Discord IDs but different server
    let result2 = name_service
        .bulk_create_names(yaml_content, server2_id.clone())
        .await
        .expect("Failed to bulk create names for server2");

    let (created_count2, skipped_count2, errors2) = result2;
    assert_eq!(created_count2, 2); // Should succeed since server is different
    assert_eq!(skipped_count2, 0);
    assert!(errors2.is_empty());

    // Verify all names exist
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert_eq!(all_names.len(), 4); // 2 per server

    // Check names in server1
    let server1_names: Vec<_> = all_names
        .iter()
        .filter(|name| name.server_id() == server1_id)
        .collect();
    assert_eq!(server1_names.len(), 2);

    // Check names in server2
    let server2_names: Vec<_> = all_names
        .iter()
        .filter(|name| name.server_id() == server2_id)
        .collect();
    assert_eq!(server2_names.len(), 2);
}

#[tokio::test]
async fn can_bulk_create_names_with_single_entry() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Test with single entry YAML
    let yaml_content = "123456789: \"SingleUser\"";
    let server_id = "test-server".to_string();

    let result = name_service
        .bulk_create_names(yaml_content, server_id.clone())
        .await
        .expect("Failed to bulk create single name");

    let (created_count, skipped_count, errors) = result;
    assert_eq!(created_count, 1);
    assert_eq!(skipped_count, 0);
    assert!(errors.is_empty());

    // Verify the name was created
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert_eq!(all_names.len(), 1);
    let created_name = &all_names[0];
    assert_eq!(created_name.discord_id(), 123456789);
    assert_eq!(created_name.name(), "SingleUser");
    assert_eq!(created_name.server_id(), server_id);
}

#[tokio::test]
async fn can_handle_bulk_create_with_special_characters_in_names() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Test with names containing special characters
    let yaml_content = r#"
123456789: "Alice O'Connor"
987654321: "José María"
555666777: "王小明"
444333222: "user@domain.com"
111000999: "🎮 GamerTag"
"#;
    let server_id = "test-server".to_string();

    let result = name_service
        .bulk_create_names(yaml_content, server_id.clone())
        .await
        .expect("Failed to bulk create names with special characters");

    let (created_count, skipped_count, errors) = result;
    assert_eq!(created_count, 5);
    assert_eq!(skipped_count, 0);
    assert!(errors.is_empty());

    // Verify all names were created correctly with special characters preserved
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");

    assert_eq!(all_names.len(), 5);

    // Check that each expected name exists with proper special character handling
    let expected_names = vec![
        (123456789u64, "Alice O'Connor"),
        (987654321u64, "José María"),
        (555666777u64, "王小明"),
        (444333222u64, "user@domain.com"),
        (111000999u64, "🎮 GamerTag"),
    ];

    for (expected_discord_id, expected_name) in expected_names {
        let found_name = all_names
            .iter()
            .find(|name| {
                name.discord_id() == expected_discord_id
                    && name.name() == expected_name
                    && name.server_id() == server_id
            })
            .unwrap_or_else(|| {
                panic!(
                    "Expected name with Discord ID {} and name '{}' not found",
                    expected_discord_id, expected_name
                )
            });
        assert!(found_name.id() > 0);
    }
}

#[tokio::test]
async fn can_bulk_delete_names() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create multiple names to delete
    let names_data = vec![
        (123456789u64, "User1".to_string(), "server1".to_string()),
        (987654321u64, "User2".to_string(), "server1".to_string()),
        (555666777u64, "User3".to_string(), "server2".to_string()),
    ];

    let mut created_ids = Vec::new();
    for (discord_id, name, server_id) in names_data {
        let created_name = name_service
            .create_name(discord_id, name, server_id)
            .await
            .expect("Failed to create name");
        created_ids.push(created_name.id());
    }

    // Verify all names were created
    let all_names_before = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert_eq!(all_names_before.len(), 3);

    // Delete two of the three names
    let ids_to_delete = &created_ids[0..2];
    let result = name_service
        .bulk_delete_names(ids_to_delete)
        .await
        .expect("Failed to bulk delete names");

    let (deleted_count, failed_deletes) = result;
    assert_eq!(deleted_count, 2);
    assert!(failed_deletes.is_empty());

    // Verify only one name remains
    let all_names_after = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert_eq!(all_names_after.len(), 1);
    assert_eq!(all_names_after[0].id(), created_ids[2]);
}

#[tokio::test]
async fn can_handle_bulk_delete_with_nonexistent_ids() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create one name
    let created_name = name_service
        .create_name(123456789, "TestUser".to_string(), "server1".to_string())
        .await
        .expect("Failed to create name");

    // Try to delete the existing name and a non-existent one
    let ids_to_delete = vec![created_name.id(), 99999];
    let result = name_service
        .bulk_delete_names(&ids_to_delete)
        .await
        .expect("Failed to bulk delete names");

    let (deleted_count, failed_deletes) = result;
    assert_eq!(deleted_count, 1); // Only the existing name was deleted
    assert_eq!(failed_deletes.len(), 1); // One failure for the non-existent ID
    assert!(failed_deletes[0].contains("99999"));
    assert!(failed_deletes[0].contains("not found"));

    // Verify no names remain
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert!(all_names.is_empty());
}

#[tokio::test]
async fn can_handle_bulk_delete_with_empty_list() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create one name first
    let _created_name = name_service
        .create_name(123456789, "TestUser".to_string(), "server1".to_string())
        .await
        .expect("Failed to create name");

    // Try to delete with empty list
    let ids_to_delete: Vec<u32> = vec![];
    let result = name_service
        .bulk_delete_names(&ids_to_delete)
        .await
        .expect("Failed to bulk delete names");

    let (deleted_count, failed_deletes) = result;
    assert_eq!(deleted_count, 0);
    assert!(failed_deletes.is_empty());

    // Verify the original name still exists
    let all_names = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert_eq!(all_names.len(), 1);
}

#[tokio::test]
async fn can_handle_bulk_delete_all_names() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create multiple names
    let names_data = vec![
        (123456789u64, "User1".to_string(), "server1".to_string()),
        (987654321u64, "User2".to_string(), "server1".to_string()),
        (555666777u64, "User3".to_string(), "server2".to_string()),
        (444333222u64, "User4".to_string(), "server2".to_string()),
        (111222333u64, "User5".to_string(), "server3".to_string()),
    ];

    let mut created_ids = Vec::new();
    for (discord_id, name, server_id) in names_data {
        let created_name = name_service
            .create_name(discord_id, name, server_id)
            .await
            .expect("Failed to create name");
        created_ids.push(created_name.id());
    }

    // Verify all names were created
    let all_names_before = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert_eq!(all_names_before.len(), 5);

    // Delete all names
    let result = name_service
        .bulk_delete_names(&created_ids)
        .await
        .expect("Failed to bulk delete all names");

    let (deleted_count, failed_deletes) = result;
    assert_eq!(deleted_count, 5);
    assert!(failed_deletes.is_empty());

    // Verify no names remain
    let all_names_after = name_service
        .get_all_names()
        .await
        .expect("Failed to get all names");
    assert!(all_names_after.is_empty());
}

#[tokio::test]
async fn can_get_names_by_server() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create names for different servers
    let server1_id = "server1".to_string();
    let server2_id = "server2".to_string();

    let name1 = name_service
        .create_name(123456789, "Alice".to_string(), server1_id.clone())
        .await
        .expect("Failed to create name1");

    let name2 = name_service
        .create_name(987654321, "Bob".to_string(), server1_id.clone())
        .await
        .expect("Failed to create name2");

    let name3 = name_service
        .create_name(555666777, "Charlie".to_string(), server2_id.clone())
        .await
        .expect("Failed to create name3");

    // Get names for server1
    let server1_names = name_service
        .get_names_by_server(&server1_id)
        .await
        .expect("Failed to get names for server1");

    assert_eq!(server1_names.len(), 2);
    assert!(server1_names.contains(&name1));
    assert!(server1_names.contains(&name2));
    assert!(!server1_names.contains(&name3));

    // Get names for server2
    let server2_names = name_service
        .get_names_by_server(&server2_id)
        .await
        .expect("Failed to get names for server2");

    assert_eq!(server2_names.len(), 1);
    assert!(server2_names.contains(&name3));
    assert!(!server2_names.contains(&name1));
    assert!(!server2_names.contains(&name2));
}

#[tokio::test]
async fn can_get_empty_list_for_nonexistent_server() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create a name for a specific server
    name_service
        .create_name(
            123456789,
            "TestUser".to_string(),
            "existing-server".to_string(),
        )
        .await
        .expect("Failed to create name");

    // Query for a non-existent server
    let names = name_service
        .get_names_by_server("nonexistent-server")
        .await
        .expect("Failed to get names for nonexistent server");

    assert!(names.is_empty());
}

#[tokio::test]
async fn can_get_names_by_server_with_special_characters() {
    let state = setup().await.expect("Failed to setup test context");
    let name_service = NameService::new(&state.db);

    // Create names with special characters in server ID
    let special_server_id = "server-with-special!@#$%^&*()".to_string();

    let name1 = name_service
        .create_name(123456789, "Alice".to_string(), special_server_id.clone())
        .await
        .expect("Failed to create name with special server ID");

    let name2 = name_service
        .create_name(987654321, "Bob".to_string(), "normal-server".to_string())
        .await
        .expect("Failed to create name with normal server ID");

    // Get names for the special server
    let special_server_names = name_service
        .get_names_by_server(&special_server_id)
        .await
        .expect("Failed to get names for special server");

    assert_eq!(special_server_names.len(), 1);
    assert!(special_server_names.contains(&name1));
    assert!(!special_server_names.contains(&name2));
}
