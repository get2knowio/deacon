FROM python:3.9

# Set workspace directory
WORKDIR ${localWorkspaceFolder}

# Install dependencies
RUN pip install --no-cache-dir \
    flask \
    requests

# Copy source code
COPY ${localWorkspaceFolder}/src /app/src

# Set working directory
WORKDIR /app