const http = require("http");
const fs = require("fs");
const path = require("path");

const port = Number(process.env.PORT);
const host = process.env.HOST || "127.0.0.1";
const messagePath = path.join(__dirname, "message.txt");

const server = http.createServer((request, response) => {
  const body = fs.existsSync(messagePath)
    ? fs.readFileSync(messagePath, "utf8")
    : "node-http";
  response.writeHead(200, { "content-type": "text/plain" });
  response.end(body);
  if (request.url === "/shutdown") {
    server.close();
  }
});

server.listen(port, host);
