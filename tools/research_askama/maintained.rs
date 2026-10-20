use std::path::PathBuf;

use askama::Template;
use googletest::prelude::{Result, assert_that, contains_substring, gtest};

struct Name<'a> {
    id: u32,
    discord_id: &'a str,
    server_id: &'a str,
    name: &'a str,
}
macro_rules! static_template {
    ($name:ident, $path:literal) => {
        #[derive(Template)]
        #[template(path = $path)]
        struct $name;
    };
}
static_template!(Layout, "layout.html");
static_template!(Index, "index.html");
static_template!(Names, "names.html");
static_template!(AddForm, "names/add_name_form.html");
static_template!(BulkAddForm, "names/bulk_add_form.html");
static_template!(BulkDelete, "names/bulk_delete.html");
static_template!(LoginError, "login/login_error_message.html");
#[derive(Template)]
#[template(path = "login.html")]
struct Login<'a> {
    username: Option<&'a str>,
}
#[derive(Template)]
#[template(path = "welcome/call_to_action.html")]
struct Welcome<'a> {
    username: Option<&'a str>,
}
#[derive(Template)]
#[template(path = "login/login_success.html")]
struct LoginSuccess<'a> {
    name: &'a str,
}
#[derive(Template)]
#[template(path = "names/error_message.html")]
struct ErrorMessage<'a> {
    message: &'a str,
}
#[derive(Template)]
#[template(path = "names/names_table.html")]
struct NamesTable<'a> {
    names: &'a [Name<'a>],
}
#[derive(Template)]
#[template(path = "names/bulk_delete_table.html")]
struct DeleteTable<'a> {
    names: &'a [Name<'a>],
}
#[derive(Template)]
#[template(path = "names/name_row.html")]
struct NameRow<'a> {
    name: &'a Name<'a>,
}
#[derive(Template)]
#[template(path = "names/edit_name_form.html")]
struct EditName<'a> {
    name: &'a Name<'a>,
}
#[derive(Template)]
#[template(path = "names/bulk_add_success.html")]
struct BulkSuccess<'a> {
    created_count: usize,
    skipped_count: usize,
    errors: &'a [&'a str],
    server_id: &'a str,
    yaml_content: &'a str,
}

#[gtest]
fn render_maintained_syntax() -> Result<()> {
    let output = PathBuf::from(std::env::var("TEST_UNDECLARED_OUTPUTS_DIR")?);
    let mut records = Vec::new();
    macro_rules! record {
        ($name:expr,$value:expr) => {{
            let rendered = $value.render()?;
            std::fs::write(output.join(format!("{}.html", $name)), &rendered)?;
            records.push(($name, rendered.len()));
        }};
    }
    record!("layout", Layout);
    record!("index", Index);
    record!("names", Names);
    record!("add-form", AddForm);
    record!("bulk-add-form", BulkAddForm);
    record!("bulk-delete", BulkDelete);
    record!("login-error", LoginError);
    record!(
        "login-some",
        Login {
            username: Some("<&>")
        }
    );
    record!("login-none", Login { username: None });
    record!(
        "welcome-some",
        Welcome {
            username: Some("<&>")
        }
    );
    record!("welcome-none", Welcome { username: None });
    record!("login-success", LoginSuccess { name: "<&>" });
    record!("error-message", ErrorMessage { message: "<&>" });
    let names = [Name {
        id: 42,
        discord_id: "discord",
        server_id: "server",
        name: "<&>",
    }];
    record!("table-empty", NamesTable { names: &[] });
    record!("table-populated", NamesTable { names: &names });
    record!("delete-empty", DeleteTable { names: &[] });
    record!("delete-populated", DeleteTable { names: &names });
    record!("name-row", NameRow { name: &names[0] });
    record!("edit-name", EditName { name: &names[0] });
    record!(
        "bulk-success-zero",
        BulkSuccess {
            created_count: 0,
            skipped_count: 0,
            errors: &[],
            server_id: "server",
            yaml_content: "yaml"
        }
    );
    record!(
        "bulk-success-errors",
        BulkSuccess {
            created_count: 2,
            skipped_count: 1,
            errors: &["<&>"],
            server_id: "server",
            yaml_content: "yaml"
        }
    );
    assert_that!(
        NameRow { name: &names[0] }.render()?,
        contains_substring("&#60;&#38;&#62;")
    );
    std::fs::write(
        output.join("functional.txt"),
        "Maintained template render assertions passed.\n",
    )?;
    std::fs::write(
        output.join("renders.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    Ok(())
}
