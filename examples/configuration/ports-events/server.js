const http = require('http');
const port = process.env.PORT || 8080;

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/plain' });
  res.end('hello from deacon ports-events example\n');
});

server.listen(port, () => {
  console.log(`server listening on ${port}`);
});
