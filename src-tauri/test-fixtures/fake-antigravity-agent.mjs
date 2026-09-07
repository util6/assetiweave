import fs from "node:fs";

const recordPath = process.env.ASSETIWEAVE_FAKE_AGY_RECORD_PATH;
const mode = process.env.ASSETIWEAVE_FAKE_AGY_MODE || "success";
const args = process.argv.slice(2);

if (recordPath) {
  let content = "";
  for (const arg of args) {
    content += `${arg}\n`;
  }
  content += "--END--\n";
  fs.appendFileSync(recordPath, content);
}

const hasResume = args.includes("--conversation");

switch (mode) {
  case "auth-failure":
    console.log(JSON.stringify({ event: "init", conversation_id: "", init: { model: "fixture-model" } }));
    console.log(JSON.stringify({ event: "result", result: { conversation_id: "", status: "ERROR", error: "authentication failed or timed out" } }));
    process.exit(1);
    break;
  case "result-id-only":
    console.log(JSON.stringify({ event: "init", init: { model: "fixture-model" } }));
    console.log(JSON.stringify({ event: "step_update", step_update: { conversation_id: "", step_index: 1, state: "ACTIVE", step_type: "agent_response", text_delta: "result id response" } }));
    console.log(JSON.stringify({ event: "result", result: { conversation_id: "RESULT_ONLY_CONVERSATION_ID", status: "SUCCESS", response: "result id response" } }));
    process.exit(0);
    break;
  case "unknown":
    console.log(JSON.stringify({ event: "future_event", future: { opaque: "ignored" } }));
    break;
  case "malformed":
    console.log(JSON.stringify({ event: "init", conversation_id: "REAL_CONVERSATION_ID", init: { model: "fixture-model" } }));
    process.stdout.write("{not-json\n");
    process.exit(0);
    break;
  case "hang":
    console.log(JSON.stringify({ event: "init", conversation_id: "REAL_CONVERSATION_ID", init: { model: "fixture-model" } }));
    setInterval(() => {}, 30_000);
    break;
}

if (mode !== "hang") {
  const response = hasResume ? "resumed fixture response" : "first fixture response";
  console.log(JSON.stringify({ event: "init", conversation_id: "REAL_CONVERSATION_ID", init: { model: "fixture-model" } }));
  console.log(JSON.stringify({ event: "step_update", step_update: { conversation_id: "REAL_CONVERSATION_ID", step_index: 2, state: "ACTIVE", step_type: "agent_response", text_delta: response } }));
  console.log(JSON.stringify({ event: "step_update", step_update: { conversation_id: "REAL_CONVERSATION_ID", step_index: 3, state: "ACTIVE", step_type: "tool", tool_name: "read_fixture", tool_info: { name: "read fixture", parameters: { secret: "RAW_TOOL_SECRET" } } } }));
  console.log(JSON.stringify({ event: "step_update", step_update: { conversation_id: "REAL_CONVERSATION_ID", step_index: 3, state: "DONE", step_type: "tool", tool_info: { name: "read fixture", output: "RAW_TOOL_SECRET" } } }));
  console.log(JSON.stringify({ event: "result", result: { conversation_id: "REAL_CONVERSATION_ID", status: "SUCCESS", response } }));
}
