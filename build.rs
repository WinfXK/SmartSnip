//! 构建脚本：把 images/icon.ico 编译为 Windows 资源对象并链接进 exe（无需 rc.exe）。

use std::num::NonZeroU16;
use std::path::PathBuf;

use win_resource_builder::image::Icon;
use win_resource_builder::resource::{CoffMachine, LanguageId, ResourceFile};
use win_resource_builder::version_info::{Version, VersionInfo};

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

    // 文件属性：版本 / 产品名 / 公司 / 版权（exe 右键属性可见）
    let version = Version::new(1, 0, 0, 0);
    let info = VersionInfo::options()
        .company_name("Winfxk")
        .file_description("SmartSnip 轻量级 Windows 常驻截图工具")
        .legal_copyright("Copyright (c) 2026 Winfxk (冰月)")
        .internal_name("SmartSnip")
        .create("SmartSnip", "SmartSnip.exe", version)
        .expect("构建版本信息失败");
    file.insert_version_info(&info)
        .expect("写入版本资源失败");
    let machine = CoffMachine::cargo_target().expect("无法确定目标架构");
    file.write_coff(std::fs::File::create(&object).expect("创建资源对象失败"), machine)
        .expect("写出资源对象失败");

    println!("cargo:rustc-link-arg-bins={}", object.display());
}
