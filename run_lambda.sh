#!/bin/sh
unzip -o lambda.zip -d /tmp/lambda && \
  docker run -i --rm  -e DISCORD_WEBHOOKS=$(printf $DISCORD_WEBHOOKS) -v /tmp/lambda:/var/task lambci/lambda:provided
