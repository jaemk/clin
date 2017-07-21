FROM jaemk/i686-unknown-linux-gnu:v0.2.0

RUN dpkg --add-architecture i386 && \
    apt-get update && apt-get upgrade -y && \
    apt-get install libdbus-1-dev:i386 -y
