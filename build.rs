//! 构建脚本：把 images/icon.ico 编译为 Windows 资源对象并链接进 exe（无需 rc.exe）。

use std::num::NonZeroU16;
use std::path::PathBuf;

use win_resource_builder::image::Icon;
use win_resource_builder::resource::{CoffMachine, LanguageId, ResourceFile};

fn main() {
    println!("cargo:rerun-if-changed=images/icon.ico");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR 未设置"));
    let object = out_dir.join("smart_snip_resources.obj");

    let icon = Icon::options()
        .create_from_file("images/icon.ico")
        .expect("解析 images/icon.ico 失败");
    let mut file = ResourceFile::new();
    file.insert_icon(NonZeroU16::MIN, LanguageId::NEUTRAL, &icon)
        .expect("写入图标资源失败");
    let machine = CoffMachine::cargo_target().expect("无法确定目标架构");
    file.write_coff(std::fs::File::create(&object).expect("创建资源对象失败"), machine)
        .expect("写出资源对象失败");

    println!("cargo:rustc-link-arg-bins={}", object.display());
}
