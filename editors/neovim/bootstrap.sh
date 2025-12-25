# todo: it is in very early stage, so we are doing dirty things.

# python3 ./spec/main.py

DOCKER_ARGS=
if [ "$1" = "test" ]; then
    DOCKER_ARGS="python3 ./spec/main.py"
elif [ "$1" = "bash" ]; then
    DOCKER_ARGS="bash"
elif [ "$1" = "editor" ]; then
    DOCKER_ARGS="nvim ."
else
    echo "Usage: $0 [test|bash|editor]"
    exit 1
fi

(cd ../.. && docker build -t myriaddreamin/tinymist:0.14.6-rc2 .)
(cd samples && docker build -t myriaddreamin/tinymist-nvim:0.14.6-rc2 -f lazyvim-dev/Dockerfile .)
docker run --rm -it \
  -v $PWD/../../tests/workspaces:/home/runner/dev/workspaces \
  -v $PWD:/home/runner/dev \
  -v $PWD/target/.local:/home/runner/.local \
  -v $PWD/target/.cache:/home/runner/.cache \
  -w /home/runner/dev myriaddreamin/tinymist-nvim:0.14.6-rc2 \
  $DOCKER_ARGS
