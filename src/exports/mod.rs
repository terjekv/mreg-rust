use minijinja::Environment;

pub fn build_environment() -> Environment<'static> {
    Environment::new()
}
