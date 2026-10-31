// 所有后端调用的唯一出口。
// 旧代码：invoke("settings_get", {...})；新代码：api.call("settings_get", {...})。
// 想加日志、超时、桩测试，只改这里；命令名与 Rust #[tauri::command] 一一对应。
const { invoke: __invoke } = window.__TAURI__.core;

const api = {
  call(name, args) {
    return __invoke(name, args);
  },
};
