# The virtual display of the end-to-end test. scripts/check-e2e.sh builds and runs this image.
#
# The first stage installs the display server. The image gets only the server and the files it
# loads, because its packages also bring graphics drivers of several hundred megabytes that it
# never uses.
FROM debian:13.7-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 AS packages
RUN apt-get update && apt-get install --yes --no-install-recommends xvfb busybox-static
# ldd prints a library as "name => path (address)" and the loader as "path (address)".
RUN mkdir --mode=1777 --parents /display/tmp \
  && ldd /usr/bin/Xvfb /usr/bin/xkbcomp \
    | awk '$2 == "=>" { print $3 } $1 ~ "^/" && $2 ~ "^\\(" { print $1 }' | sort --unique \
    | xargs cp --parents --target-directory=/display /bin/busybox /usr/bin/Xvfb /usr/bin/xkbcomp \
  && cp --parents --recursive --target-directory=/display /usr/share/X11/xkb \
  && ln --symbolic busybox /display/bin/sh

FROM scratch
COPY --from=packages /display /
USER 1000:1000
EXPOSE 6000
# The display ends itself after 900 seconds, which is longer than the time limit of the gate,
# so a gate that was killed cannot leave it behind. It accepts every client, because the gate
# publishes it on the loopback address only. It offers no shared memory, because that cannot
# cross the container boundary.
CMD ["/bin/busybox", "timeout", "900", "Xvfb", "-displayfd", "1", "-screen", "0", "1280x800x24", "-listen", "tcp", "-nolisten", "unix", "-ac", "-extension", "MIT-SHM"]
