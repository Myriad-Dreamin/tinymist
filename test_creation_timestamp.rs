use std::sync::Arc;
use tinymist_world::{CompilerFeat, CompilerUniverse};
use tinymist_world::system::SystemCompilerFeat;
use tinymist_world::entry::EntryState;
use typst::{Features, World, foundations::Dict};
use typst::utils::LazyHash;

#[test]
fn test_creation_timestamp_today() {
    // Test that the World's today() method respects the creation timestamp
    let entry = EntryState::new_rooted("/tmp".into(), None);
    let features = Features::default();
    let inputs = Arc::new(LazyHash::new(Dict::new()));
    let vfs = tinymist_world::vfs::Vfs::new(
        Arc::new(tinymist_world::package::DummyRegistry::default()),
        tinymist_world::system::SystemAccessModel {},
    );
    let registry = Arc::new(tinymist_world::package::DummyRegistry::default());
    let font_resolver = Arc::new(tinymist_world::font::system::SystemFontResolver::new());
    
    // Create universe with fixed timestamp (1979-12-31 equivalent)
    let fixed_timestamp = Some(315446400); // 1979-12-31 in Unix timestamp
    let universe = CompilerUniverse::<SystemCompilerFeat>::new_raw(
        entry,
        features,
        Some(inputs),
        vfs,
        registry,
        font_resolver,
        fixed_timestamp,
    );
    
    let world = universe.snapshot();
    
    // Check that today() returns the fixed date
    if let Some(date) = world.today(None) {
        // The date should be 1979-12-31
        assert_eq!(date.year(), 1979);
        assert_eq!(date.month(), 12);
        assert_eq!(date.day(), 31);
    } else {
        panic!("today() returned None");
    }
}