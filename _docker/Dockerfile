FROM johnnysmitherson/crack-dev:base

RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    imagemagick
VOLUME /root
WORKDIR /workspace

EXPOSE 22

CMD ["/usr/sbin/sshd", "-D"]