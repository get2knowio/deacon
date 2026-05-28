FROM alpine:3.18

# Image declares its own long-running CMD. With overrideCommand: false,
# deacon must NOT replace this with `sleep infinity`.
CMD ["sh", "-c", "echo image-cmd-ran > /tmp/image.cmd && exec sleep infinity"]
