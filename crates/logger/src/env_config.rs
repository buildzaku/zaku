use log::LevelFilter;

pub struct EnvFilter {
    pub level_global: Option<LevelFilter>,
    pub directive_names: Vec<String>,
    pub directive_levels: Vec<LevelFilter>,
}

pub fn parse(filter: &str) -> anyhow::Result<EnvFilter> {
    let mut max_level = None;
    let mut directive_names = Vec::new();
    let mut directive_levels = Vec::new();

    for directive in filter.split(',') {
        match directive.split_once('=') {
            Some((name, level)) => {
                anyhow::ensure!(!level.contains('='), "Invalid directive: {directive}");

                let level = parse_level(level.trim())?;
                directive_names.push(name.trim().trim_end_matches(".rs").to_string());
                directive_levels.push(level);
            }
            None => {
                let Ok(level) = parse_level(directive.trim()) else {
                    directive_names.push(directive.trim().trim_end_matches(".rs").to_string());
                    directive_levels.push(LevelFilter::max());
                    continue;
                };
                anyhow::ensure!(max_level.is_none(), "Cannot set multiple max levels");
                max_level.replace(level);
            }
        };
    }

    Ok(EnvFilter {
        level_global: max_level,
        directive_names,
        directive_levels,
    })
}

fn parse_level(level: &str) -> anyhow::Result<LevelFilter> {
    if level.eq_ignore_ascii_case("TRACE") {
        return Ok(LevelFilter::Trace);
    } else if level.eq_ignore_ascii_case("DEBUG") {
        return Ok(LevelFilter::Debug);
    } else if level.eq_ignore_ascii_case("INFO") {
        return Ok(LevelFilter::Info);
    } else if level.eq_ignore_ascii_case("WARN") {
        return Ok(LevelFilter::Warn);
    } else if level.eq_ignore_ascii_case("ERROR") {
        return Ok(LevelFilter::Error);
    } else if level.eq_ignore_ascii_case("OFF") || level.eq_ignore_ascii_case("NONE") {
        return Ok(LevelFilter::Off);
    } else {
        anyhow::bail!("Invalid level: {level}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_level() {
        let input = "info";
        let filter = parse(input).unwrap();

        assert_eq!(filter.level_global.unwrap(), LevelFilter::Info);
        assert!(filter.directive_names.is_empty());
        assert!(filter.directive_levels.is_empty());
    }

    #[test]
    fn test_directive_level() {
        let input = "test_module=debug";
        let filter = parse(input).unwrap();

        assert_eq!(filter.level_global, None);
        assert_eq!(filter.directive_names, vec!["test_module".to_string()]);
        assert_eq!(filter.directive_levels, vec![LevelFilter::Debug]);
    }

    #[test]
    fn test_global_level_and_directive_level() {
        let input = "info,test_module=debug";
        let filter = parse(input).unwrap();

        assert_eq!(filter.level_global.unwrap(), LevelFilter::Info);
        assert_eq!(filter.directive_names, vec!["test_module".to_string()]);
        assert_eq!(filter.directive_levels, vec![LevelFilter::Debug]);
    }

    #[test]
    fn test_global_level_and_bare_module() {
        let input = "info,test_module";
        let filter = parse(input).unwrap();

        assert_eq!(filter.level_global.unwrap(), LevelFilter::Info);
        assert_eq!(filter.directive_names, vec!["test_module".to_string()]);
        assert_eq!(filter.directive_levels, vec![LevelFilter::max()]);
    }

    #[test]
    fn test_error_on_multiple_max_levels() {
        let input = "info,warn";
        let result = parse(input);

        assert!(result.is_err());
    }

    #[test]
    fn test_error_on_invalid_level() {
        let input = "test_module=foobar";
        let result = parse(input);

        assert!(result.is_err());
    }
}
