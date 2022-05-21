use static_files::NpmBuild;

fn main() -> std::io::Result<()> {
    NpmBuild::new("../../")
        .executable("yarn")
        .install()?
        .run("build")?
        .target("../../dist")
        .change_detection()
        .to_resource_dir()
        .build()
}