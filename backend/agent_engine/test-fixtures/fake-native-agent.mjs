import fs from "node:fs";

const recordPath = process.env.ASSETIWEAVE_FAKE_NATIVE_RECORD_PATH;
const args = process.argv.slice(2);

if (recordPath) {
  let content = "";
  for (const arg of args) {
    content += `${arg}\n`;
  }
  content += "--END--\n";
  fs.appendFileSync(recordPath, content);
}

console.log(JSON.stringify({ event: "result", result: { status: "SUCCESS", response: "native fixture response" } }));
