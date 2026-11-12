// Встраивает значок в exe: его показывают Проводник, ярлыки и панель задач.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico"); // попадает в ресурсы под номером 1
        resource.compile().expect("не удалось встроить значок");
    }
}
