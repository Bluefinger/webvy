use webvy_app::{
    app::ProcessorApp,
    processor::{ConfigurationProcessor, MarkdownFrontMatter, MarkdownProcessor, TeraProcessor},
};

fn main() {
    env_logger::init();

    ProcessorApp::default()
        .add_processor(ConfigurationProcessor::new("blog.toml"))
        .add_processor(MarkdownProcessor::<MarkdownFrontMatter>::default())
        .add_processor(TeraProcessor::default())
        .run();
}
