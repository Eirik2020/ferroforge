use ferroforge::dependency_registry;

dependency_registry! {
    Fugit => {
        package = "fugit",
        version = "0.3.9",
        default_features = false,
        features = [],
    },
    Defmt => {
        package = "defmt",
        version = "1.1.1",
        default_features = false,
        features = [],
    },
}
