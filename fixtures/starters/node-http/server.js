const http = require("http");

const server = http.createServer((_request, response) => {
  response.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
  response.end("Hello from __STACKYARD_NAME__\n");
});

server.listen(Number(process.env.PORT || 3000), "127.0.0.1");
