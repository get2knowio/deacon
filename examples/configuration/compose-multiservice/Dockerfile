FROM node:18-alpine
WORKDIR /workspace
COPY package.json package-lock.json* ./
RUN npm ci || true
COPY . .
EXPOSE 3000
CMD ["node", "-e", "require('http').createServer((_,res)=>res.end('ok')).listen(3000)"]
