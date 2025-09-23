FROM python:3.9
WORKDIR ${localWorkspaceFolder}
RUN pip install --no-cache-dir flask requests
COPY ${localWorkspaceFolder}/src /app/src
WORKDIR /app
