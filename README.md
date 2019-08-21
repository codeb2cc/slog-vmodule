[![Build Status](https://travis-ci.org/codeb2cc/slog-vmodule.svg?branch=master)](https://travis-ci.org/codeb2cc/slog-vmodule)
[![codecov](https://codecov.io/gh/codeb2cc/slog-vmodule/branch/master/graph/badge.svg)](https://codecov.io/gh/codeb2cc/slog-vmodule)


# slog-vmodule - Module based level filter Drain for [slog-rs]


### Features
Filter Drain provides fine-grained control over logging at the module(logger name) level. Examples: With flag `foo=debug,bar=error` and default level INFO, a debug log will only be drain if it's from *foo* but not from *bar*. And logs from *bar* must have a at least ERROR level to pass. Other logs without module name continue to adopt the default INFO setting.

### Usage

```
    // ... 

    let vmodule: HashMap<String, Level> = [
        ("foo".to_owned(), Level::Debug),
        ("bar".to_owned(), Level::Error),
    ]
    .iter()
    .cloned()
    .collect();
    // Or parse from flag string
    //let vmodule: HashMap<String, Level> = ModLevelFilterConfig("foo=debug,bar=error".to_string()).into();

    // Module name key("module" here) is configurable. Use whatever you like.
    let filter =
        ModLevelFilter::new(drain, "module".to_owned(), Level::Warning, vmodule).fuse();

    // Logger for different modules
    let root_log = Logger::root(filter.fuse(), o!());
    let foo_log = root_log.new(o!("module" => "foo"));
    let bar_log = root_log.new(o!("module" => "bar"));

    info!(root_log, "Not Pass");
    info!(foo_log, "Pass");
    info!(bar_log, "Not Pass");

    // ...
```


[slog-rs]: https://github.com/slog-rs/slog
