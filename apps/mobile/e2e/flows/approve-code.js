// Approves the code on screen (copied by the flow) as a signed-in device would.
const code = maestro.copiedText.replace(/\s/g, "");
const response = http.post(`${SERVER}/api/auth/quick-connect/approve`, {
  headers: { "Content-Type": "application/json", Authorization: `Bearer ${TOKEN}` },
  body: JSON.stringify({ code: code }),
});
if (response.status !== 204)
  throw new Error(`approve ${code}: ${response.status} ${response.body}`);
