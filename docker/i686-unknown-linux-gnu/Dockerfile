FROM ubuntu:trusty

RUN dpkg --add-architecture i386 && \
    apt-get update && apt-get upgrade -y && \
    apt-get install build-essential gcc-multilib libdbus-1-dev:i386 -y
