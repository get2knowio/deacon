# syntax=docker/dockerfile:1
FROM node:18-alpine

# Example: Mount GitHub token as secret during npm install
RUN --mount=type=secret,id=github_token \
    if [ -f /run/secrets/github_token ]; then \
        echo "Secret file exists, configuring npm..." && \
        npm config set //npm.pkg.github.com/:_authToken=$(cat /run/secrets/github_token) && \
        echo "Would install private packages here..." && \
        npm config delete //npm.pkg.github.com/:_authToken; \
    else \
        echo "No secret provided, skipping private package installation"; \
    fi

# Copy application files
WORKDIR /app
COPY package*.json ./
RUN npm install

COPY . .

CMD ["npm", "start"]
