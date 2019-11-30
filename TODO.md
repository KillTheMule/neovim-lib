* Make Buffers/Windows/??? own a Requester, so methods on it don't need to get &Neovim passed

* Make the io loop own a requester. Make the handler methods take a requester. The loop passes
  down a clone so the methods can make requests directly
