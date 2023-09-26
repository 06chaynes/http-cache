# Cache Modes

When constructing a new instance of `HttpCache`, you must specify a cache mode. The cache mode determines how the cache will behave in certain situations. These modes are similar to [make-fetch-happen cache options](https://github.com/npm/make-fetch-happen#--optscache). The available cache modes are:

- `Default`: This mode will inspect the HTTP cache on the way to the network. If there is a fresh response it will be used. If there is a stale response a conditional request will be created, and a normal request otherwise. It then updates the HTTP cache with the response. If the revalidation request fails (for example, on a 500 or if you're offline), the stale response will be returned.

- `NoStore`: This mode will ignore the HTTP cache on the way to the network. It will always create a normal request, and will never cache the response.

- `Reload`: This mode will ignore the HTTP cache on the way to the network. It will always create a normal request, and will update the HTTP cache with the response.

- `NoCache`: This mode will create a conditional request if there is a response in the HTTP cache and a normal request otherwise. It then updates the HTTP cache with the response.

- `ForceCache`: This mode will inspect the HTTP cache on the way to the network. If there is a cached response it will be used regardless of freshness. If there is no cached response it will create a normal request, and will update the cache with the response.

- `OnlyIfCached`: This mode will inspect the HTTP cache on the way to the network. If there is a cached response it will be used regardless of freshness. If there is no cached response it will return a `504 Gateway Timeout` error.

- `IgnoreRules`: This mode will ignore the HTTP headers and always store a response given it was a 200 status code. It will also ignore the staleness when retrieving a response from the cache, so expiration of the cached response will need to be handled manually. If there was no cached response it will create a normal request, and will update the cache with the response.
