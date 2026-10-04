#[cfg_attr(feature = "selected", path = "selected.rs")]
mod feature;
#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(not(unix), path = "portable.rs")]
mod platform;
#[cfg_attr(true, cfg_attr(feature = "selected", path = "nested.rs"))]
mod nested;
#[cfg_attr(true, cfg(false))]
mod absent;
#[cfg_attr(test, path = "test_inline")]
mod inline { #[cfg(not(test))] mod prod; }
#[cfg_attr(unix, path = "unix_scope")]
#[cfg_attr(not(unix), path = "other_scope")]
mod scope {
    #[cfg(unix)] mod unix_only;
    #[cfg(not(unix))] mod other_only;
}
#[cfg_attr(unix, path = "unix_outer.rs")]
#[cfg_attr(not(unix), path = "other_outer.rs")]
mod outer;
#[cfg_attr(unix, path = "shared.rs")]
#[cfg_attr(not(unix), path = "shared.rs")]
mod common;
#[cfg_attr(test, path = "test_impl.rs")]
#[cfg_attr(not(test), path = "prod_impl.rs")]
mod mode;
