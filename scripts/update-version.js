#!/usr/bin/env node

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

// 获取 __dirname 的 ES 模块等价物
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('❌ 请提供版本号，例如: yarn version:update 0.2.0');
  process.exit(1);
}

const newVersion = args[0];

// 验证版本号格式
if (!/^\d+\.\d+\.\d+$/.test(newVersion)) {
  console.error('❌ 版本号格式错误，应该是 x.y.z 格式（如 0.2.0）');
  process.exit(1);
}

console.log(`🔄 正在更新版本号到 ${newVersion}...\n`);

// 1. 更新 package.json
const packageJsonPath = path.join(__dirname, '..', 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const oldVersion = packageJson.version;
packageJson.version = newVersion;
fs.writeFileSync(packageJsonPath, JSON.stringify(packageJson, null, 2) + '\n');
console.log(`✅ package.json: ${oldVersion} → ${newVersion}`);

// 2. 更新 src-tauri/Cargo.toml
const cargoTomlPath = path.join(__dirname, '..', 'src-tauri', 'Cargo.toml');
let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
const cargoVersionMatch = cargoToml.match(/version = "([^"]+)"/);
const oldCargoVersion = cargoVersionMatch ? cargoVersionMatch[1] : '未知';
cargoToml = cargoToml.replace(
  /version = "[^"]+"/,
  `version = "${newVersion}"`
);
fs.writeFileSync(cargoTomlPath, cargoToml);
console.log(`✅ src-tauri/Cargo.toml: ${oldCargoVersion} → ${newVersion}`);

// 3. 更新 src-tauri/tauri.conf.json
const tauriConfPath = path.join(__dirname, '..', 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
const oldTauriVersion = tauriConf.version;
tauriConf.version = newVersion;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
console.log(`✅ src-tauri/tauri.conf.json: ${oldTauriVersion} → ${newVersion}`);

console.log(`\n🎉 版本号已全部更新为 ${newVersion}！`);
console.log('\n💡 下一步操作：');
console.log('   1. 运行 yarn install 更新 package-lock.json');
console.log('   2. 提交更改: git add . && git commit -m "chore: bump version to ' + newVersion + '"');
