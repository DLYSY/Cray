use std::{str::FromStr, sync::OnceLock};
use uuid::Uuid;

pub static CONFIG: OnceLock<EnvConfig> = OnceLock::new();

pub fn init_config() -> Result<(), &'static str> {
    let port = std::env::var("CRAY_PORT")
        .unwrap_or("80".to_string())
        .parse::<u16>()
        .map_err(|_| "端口非法")?;

    let uuid = Uuid::from_str(
        std::env::var("CRAY_UUID")
            .map_err(|_| "找不到环境变量: CRAY_UUID")?
            .as_str(),
    )
    .map_err(|_| "CRAY_UUID 格式错误")?;

    let mode = match std::env::var("CRAY_MODE")
        .unwrap_or("stream_one".to_string())
        .as_str()
    {
        "packet_up" => XHTTPMode::packet_up,
        "stream_up" => XHTTPMode::stream_up,
        "stream_one" => XHTTPMode::stream_one,
        _ => return Err("CRAY_MODE 格式错误"),
    };

    CONFIG.set(EnvConfig {
        port: port,
        uuid: uuid,
        mode: mode,
    }).map_err(|_|"全局配置已被初始化")
}

pub struct EnvConfig {
    pub port: u16,
    pub uuid: Uuid,
    pub mode: XHTTPMode,
}

enum XHTTPMode {
    packet_up,
    stream_up,
    stream_one,
}

// use flexi_logger::{
//     Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode,
//     colored_detailed_format, detailed_format,
// };
// use log::debug;

// pub async fn run() {}

// pub fn init_log(log_level: Option<LogLevel>) -> Result<LoggerHandle, String> {
//     let logger = Logger::with(
//         log_level
//             .unwrap_or_else(|| {
//                 if cfg!(debug_assertions) {
//                     LogLevel::Debug
//                 } else {
//                     LogLevel::Info
//                 }
//             })
//             .to_loglevel(),
//     )
//     .log_to_file(
//         FileSpec::default()
//             .directory(DATA_DIR.join("logs")) //定义日志文件位置
//             .basename("ddns"),
//     ) //定义日志文件名，不包含后缀
//     .duplicate_to_stdout(Duplicate::Trace) //复制日志到控制台
//     .rotate(
//         Criterion::Age(Age::Day), // 按天轮转
//         Naming::TimestampsCustomFormat {
//             current_infix: None,
//             format: "%Y-%m-%d",
//         }, // 文件名包含日期并以天为单位轮换
//         Cleanup::KeepCompressedFiles(15), // 保留15天日志并启用压缩
//     )
//     .format_for_stdout(colored_detailed_format) //控制台输出彩色带时间的日志格式
//     .format_for_files(detailed_format) //文件中使用ANSI颜色会乱码，所以使用无颜色格式
//     .write_mode(WriteMode::Async)
//     .append() //指定日志文件为添加内容而不是覆盖重写
//     .start()
//     .map_err(|e| format!("无法创建logger句柄,回溯错误:\n{e}"))?;
//     debug!("日志初始化成功");
//     Ok(logger)
// }
