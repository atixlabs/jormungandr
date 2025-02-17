This chapter covers the node documentation, necessary to have a working system. It covers
the network, logging and storage parameters.

The node configuration uses the [YAML](https://en.wikipedia.org/wiki/YAML) format.

This is an example of a configuration file:

```YAML
storage: "/tmp/storage"
log:
  level: debug
  format: json
p2p:
  trusted_peers:
    - "/ip4/104.24.28.11/tcp/8299"
    - "/ip4/104.24.29.11/tcp/8299"
  public_address: "/ip4/127.0.0.1/tcp/8080"
  topics_of_interest:
    messages: low
    blocks: normal
```
