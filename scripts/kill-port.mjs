// 启动 vite 前自动释放被占用的开发端口，避免 "Port 1420 is already in use"。
// 仅结束占用该端口的进程（通常是上一次失败残留的 vite），不影响其它端口。
import { execSync } from "node:child_process";
import { platform } from "node:os";

const PORT = 1420;

// 返回占用该端口的进程 PID（LISTENING 状态），找不到返回 null
function getPidOnPort(port) {
  if (platform() === "win32") {
    try {
      // 直接跑 netstat，不做管道，解析放在 JS 里，避免依赖具体 shell 的 findstr
      const out = execSync("netstat -ano", {
        encoding: "utf-8",
        stdio: ["ignore", "pipe", "ignore"],
      });
      for (const line of out.split("\n")) {
        const cols = line.trim().split(/\s+/);
        // netstat 输出形如: TCP 0.0.0.0:1420 0.0.0.0:0 LISTENING 1234
        const stateIdx = cols.indexOf("LISTENING");
        if (stateIdx === -1 || !cols[stateIdx + 1]) continue;
        const local = cols[1] || "";
        // 精确匹配本地端口（避免误匹配 :14200 之类）
        if (local.endsWith(`:${port}`)) {
          return parseInt(cols[stateIdx + 1], 10);
        }
      }
    } catch {
      // netstat 无结果会抛错，忽略
    }
  } else {
    try {
      const out = execSync(`lsof -ti tcp:${port}`, {
        encoding: "utf-8",
        stdio: ["ignore", "pipe", "ignore"],
      }).trim();
      if (out) return parseInt(out.split("\n")[0], 10);
    } catch {
      // lsof 无结果会抛错，忽略
    }
  }
  return null;
}

// 同步等待若干毫秒（不依赖外部 sleep 命令）
function sleepSync(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

const pid = getPidOnPort(PORT);
if (pid) {
  console.log(`[kill-port] 端口 ${PORT} 被 PID ${pid} 占用，自动结束...`);
  try {
    if (platform() === "win32") {
      execSync(`taskkill /PID ${pid} /F`, { stdio: "ignore" });
    } else {
      process.kill(pid, "SIGKILL");
    }
  } catch (e) {
    console.warn(`[kill-port] 结束 PID ${pid} 失败: ${e.message}`);
  }
  // 等待端口真正释放（最多约 1.5s）
  for (let i = 0; i < 5; i++) {
    sleepSync(300);
    if (!getPidOnPort(PORT)) break;
  }
  console.log(`[kill-port] 端口 ${PORT} 已释放，继续启动 dev server。`);
} else {
  console.log(`[kill-port] 端口 ${PORT} 空闲，无需处理。`);
}
