# File download

## Async runtime

The lib supports eather `async-std` or `tokio` using cargo `features=[...]` option. The default is tokio but **You must set it on your own to prevent changes**

## Usage

File download provides an easy way to download multiple files.

```
let mut target_folder = DownloadFolder::new("./")
target_folder.add_file(FileToDl {
    target: "myfile.txt",
    source: "https://source.com/myfile.txt",
    check_sum: CheckSum::None,
});
target_folder.download_http2();
# if you wan't to download 5 by 5 use download_http2_by_chunk(5)
```

This lib is fully async and can use async_std or tokio (v1.X)
