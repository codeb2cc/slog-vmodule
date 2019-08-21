//! Filter records by matching their module name against a map of module-level settings and only
//! allowing for records of level high enough to pass. The module name key is configurable. And
//! it also provides a vmodule setting string parser. The settings string is a comma-separated list
//! of MODULE=LEVEL key value paris.

use std::collections::HashMap;

#[cfg(test)]
#[macro_use]
extern crate slog;

use slog::{Drain, Key, Level, OwnedKVList, Record, Result, Serializer, KV};

/// Comma-separted list of MODULE=LEVEL key value paris to configure module log level settings
#[derive(Debug, Clone)]
pub struct ModLevelFilterConfig(pub String);

/// Parse into the HashMap ModLevelFilter needed
impl Into<HashMap<String, Level>> for ModLevelFilterConfig {
    fn into(self) -> HashMap<String, Level> {
        let mut map = HashMap::<String, Level>::new();
        self.0
            .split(',')
            .map(|kv: &str| {
                if let [module, level] = kv.splitn(2, '=').collect::<Vec<&str>>().as_slice() {
                    let slog_level = match level.to_uppercase().as_str() {
                        "TRACE" => Some(Level::Trace),
                        "DEBUG" => Some(Level::Debug),
                        "INFO" => Some(Level::Info),
                        "WARN" | "WARNING" => Some(Level::Warning),
                        "ERR" | "ERROR" => Some(Level::Error),
                        "CRIT" | "CRITICAL" => Some(Level::Critical),
                        _ => None,
                    };
                    if let Some(level) = slog_level {
                        map.insert(module.to_string(), level);
                    }
                }
            })
            .for_each(drop);

        map
    }
}

struct ModLevelSerializer {
    mod_key: String,
    val: Option<String>,
}

impl Serializer for ModLevelSerializer {
    fn emit_str(&mut self, key: Key, val: &str) -> Result {
        if key == self.mod_key {
            self.val = Some(val.to_string());
        }
        Ok(())
    }

    fn emit_arguments(&mut self, _key: Key, _val: &std::fmt::Arguments) -> Result {
        Ok(())
    }
}

pub type ModLevelMap = HashMap<String, Level>;

/// `Drain` filtering records by `Record` logging level. If the record's emitter logger has module
/// name set, only records with at least given module level will pass. If the module name is not
/// set or there's no correspondent module level config, the default logging level will be used.
///
/// For more usage examples check README and test code.
pub struct ModLevelFilter<D: Drain> {
    drain: D,
    mod_key: String,
    default_level: Level,
    filters: ModLevelMap,
}

impl<D: Drain> std::panic::UnwindSafe for ModLevelFilter<D> {}
impl<D: Drain> std::panic::RefUnwindSafe for ModLevelFilter<D> {}

impl<'a, D: Drain> ModLevelFilter<D> {
    pub fn new(drain: D, mod_key: String, default_level: Level, filters: ModLevelMap) -> Self {
        ModLevelFilter {
            drain,
            mod_key,
            default_level,
            filters,
        }
    }
}

impl<'a, D: Drain> Drain for ModLevelFilter<D> {
    type Err = Option<D::Err>;
    type Ok = Option<D::Ok>;

    fn log(
        &self,
        record: &Record,
        logger_values: &OwnedKVList,
    ) -> std::result::Result<Self::Ok, Self::Err> {
        let mut level = self.default_level;
        if !self.filters.is_empty() {
            // If there's no module level config, skip iterating the logger_values. In this
            // case it becomes a `slog::LevelFilter`
            let mut ser = ModLevelSerializer {
                mod_key: self.mod_key.to_owned(),
                val: None,
            };
            logger_values.serialize(record, &mut ser).unwrap();

            if let Some(ref mod_name) = ser.val {
                // Logger has a module name
                if let Some(mod_level) = self.filters.get(mod_name) {
                    // Filter has log level setting for logger module
                    level = *mod_level;
                }
            }
        }

        if !record.level().is_at_least(level) {
            return Ok(None);
        }
        self.drain
            .log(record, logger_values)
            .map(Some)
            .map_err(Some)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt::{self, Display, Formatter};
    use std::io;
    use std::sync::{Arc, Mutex};

    use super::{ModLevelFilter, ModLevelFilterConfig};
    use slog::{Drain, Level, Logger, OwnedKVList, Record};

    const YES: &str = "YES";
    const NO: &str = "NO";

    /// Hacked logger drain from slog-kvfilter that just counts messages to make sure we have tests
    /// behaving correcly
    #[derive(Debug)]
    struct StringDrain {
        output: Arc<Mutex<Vec<String>>>,
    }

    impl<'a> Drain for StringDrain {
        type Err = io::Error;
        type Ok = ();

        fn log(&self, info: &Record, _: &OwnedKVList) -> io::Result<()> {
            let mut lo = self.output.lock().unwrap();
            let fmt = format!("{:?}", info.msg());

            if !fmt.contains(YES) && !fmt.contains(NO) {
                panic!(fmt);
            }

            (*lo).push(fmt);

            Ok(())
        }
    }

    impl<'a> Display for StringDrain {
        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            write!(f, "none")
        }
    }

    #[test]
    fn test_vmodule_config() {
        // Single module level
        let map: HashMap<String, Level> = ModLevelFilterConfig("foo=info".to_string()).into();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("foo"), Some(&Level::Info));

        // Multiple module levels
        let map: HashMap<String, Level> =
            ModLevelFilterConfig("foo=info,bar=error".to_string()).into();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("foo"), Some(&Level::Info));
        assert_eq!(map.get("bar"), Some(&Level::Error));

        // Case unsensitive log level value
        let map: HashMap<String, Level> =
            ModLevelFilterConfig("foo=err,bar=WARN".to_string()).into();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("foo"), Some(&Level::Error));
        assert_eq!(map.get("bar"), Some(&Level::Warning));

        // Ignore invalid log level
        let map: HashMap<String, Level> =
            ModLevelFilterConfig("foo=warning,bar=unknown".to_string()).into();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("foo"), Some(&Level::Warning));

        // Into empty map if config is totally invalid
        let map: HashMap<String, Level> = ModLevelFilterConfig("invalid config".to_string()).into();
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_vmodule_filter() {
        let out = Arc::new(Mutex::new(vec![]));
        let drain = StringDrain {
            output: out.clone(),
        };

        let vmodule: HashMap<String, Level> = [
            ("foo".to_owned(), Level::Debug),
            ("bar".to_owned(), Level::Error),
        ]
        .iter()
        .cloned()
        .collect();
        let filter =
            ModLevelFilter::new(drain, "module".to_owned(), Level::Warning, vmodule).fuse();

        // Logger for different modules
        let root_log = Logger::root(filter.fuse(), o!());
        let foo_log = root_log.new(o!("module" => "foo"));
        let bar_log = root_log.new(o!("module" => "bar"));
        let foobar_log = root_log.new(o!("module" => "foobar"));

        debug!(root_log, "NO: filtered, default filter level is Warning");
        debug!(foo_log, "YES: unfiltered, foo's filter level is Debug");
        debug!(bar_log, "NO: filtered, bar's filter level is Error");
        debug!(foobar_log, "NO: filtered, same filter level as root");

        info!(root_log, "NO: filtered");
        info!(foo_log, "YES: unfiltered");
        info!(bar_log, "NO: filtered");
        info!(foobar_log, "NO: filtered");

        warn!(root_log, "YES: unfiltered, default filter level Warning");
        warn!(foo_log, "YES: unfiltered");
        warn!(bar_log, "NO: filtered, higher level than default");
        warn!(foobar_log, "YES: unfiltered, same as root");

        error!(root_log, "YES: unfiltered");
        error!(foo_log, "YES: unfiltered");
        error!(bar_log, "YES: unfiltered, meets bar's filter level");
        error!(foobar_log, "YES: unfiltered");

        println!("resulting output: {:#?}", *out.lock().unwrap());

        assert_eq!(out.lock().unwrap().len(), 9);
    }
}
