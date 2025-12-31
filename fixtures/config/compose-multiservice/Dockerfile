FROM node:18-alpine

WORKDIR /workspace

# Install basic dependencies
RUN apk add --no-cache git bash curl

# Create a simple test app
RUN echo 'console.log("Hello from multi-service app!");' > app.js

EXPOSE 3000

CMD ["node", "app.js"]