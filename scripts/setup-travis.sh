#/bin/sh -x

if [ "`uname -s`" = "Darwin" ]; then
  echo "skipping cmake upgrade on OSX"
else
  echo "yes" | sudo add-apt-repository ppa:kalakris/cmake
  sudo apt-get update -qq
  sudo apt-get install cmake
fi
