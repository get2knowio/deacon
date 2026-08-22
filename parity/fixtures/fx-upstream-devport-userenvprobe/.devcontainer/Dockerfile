FROM alpine:3.19

RUN adduser foo --disabled-password --uid 1000

RUN adduser foo2 --disabled-password --uid 1001

RUN adduser bar --disabled-password --uid 1002

RUN echo export PROFILE_SEEN=true >>/home/bar/.profile
