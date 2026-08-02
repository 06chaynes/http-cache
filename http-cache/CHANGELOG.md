<!DOCTYPE html>
<html lang="en" id="oranda" class="dark axo">
  <head>
    <title>http-cache</title>
    
    
      <link rel="icon" href="/favicon.ico" />
    
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    
      <meta name="description" content="An HTTP caching middleware" />
      <meta property="og:description" content="An HTTP caching middleware" />
    
    <meta property="og:type" content="website" />
    <meta property="og:title" content="http-cache" />
    
    
    
    <meta http-equiv="Permissions-Policy" content="interest-cohort=()" />
    <link rel="stylesheet" href="/oranda-v0.6.1.css" />
    
      <link rel="stylesheet" href="/custom.css" />
    
    
  </head>
  <body>
    <div class="container">
      <div class="page-body">
        
          <div class="repo_banner">
            <a href="https://github.com/06chaynes/http-cache">
              <div class="github-icon" aria-hidden="true"></div>
              Check out our GitHub!
            </a>
          </div>
        

        <main>
          <header>
            
            <h1 class="title">http-cache</h1>
            
  <nav class="nav">
    <ul>
      <li><a href="/">Home</a></li>

      
        
          <li><a href="/./http-cache/CHANGELOG/">http-cache changelog</a></li>
        
          <li><a href="/./http-cache-reqwest/README/">http-cache-reqwest</a></li>
        
          <li><a href="/./http-cache-reqwest/CHANGELOG/">http-cache-reqwest changelog</a></li>
        
          <li><a href="/./http-cache-surf/README/">http-cache-surf</a></li>
        
          <li><a href="/./http-cache-surf/CHANGELOG/">http-cache-surf changelog</a></li>
        
          <li><a href="/./http-cache-quickcache/README/">http-cache-quickcache</a></li>
        
          <li><a href="/./http-cache-quickcache/CHANGELOG/">http-cache-quickcache changelog</a></li>
        
      

      

      
        <li><a href="/book/">Docs</a></li>
      

      
        <li><a href="/funding/">Funding</a></li>
      

      
    </ul>
  </nav>

          </header>

          
  
    <h1>Changelog</h1>
<h2>[1.0.0-alpha.7] - 2026-07-22</h2>
<h3>Added</h3>
<ul>
<li><code>RedbManager</code> cache backend using redb for persistent, synchronous on-disk caching that requires no async runtime</li>
<li><code>manager-redb</code> feature flag for enabling <code>RedbManager</code></li>
<li><code>StreamingCacheManager::update_metadata</code> for refreshing a stored entry's headers, policy, and user metadata without touching the body file. This is a required method, so existing implementations of the trait must add it.</li>
<li><code>CachedRequestMethod</code> and <code>CacheEntryToken</code> response extensions, used by the streaming orchestrator to tell a manager which request method produced a response and which stored entry a cached response was served from</li>
</ul>
<h3>Changed</h3>
<ul>
<li>MSRV bumped from 1.88.0 to 1.89.0</li>
<li>Default cache manager changed from <code>manager-cacache</code> to <code>manager-redb</code>. <code>manager-cacache</code> (and <code>CACacheManager</code>) is now opt-in — enable the <code>manager-cacache</code> feature to keep using it. The <code>cacache</code> crate is no longer maintained upstream.</li>
<li><code>RedbManager</code> holds an exclusive file lock on its database: a second manager (or process) opening the same path fails at construction. Share one instance via <code>Arc</code> instead. This differs from the previous default <code>CACacheManager</code>, which allowed concurrent access to one cache directory.</li>
<li><code>RedbManager</code> stores are batched for durability: commits are flushed to disk every 64 writes (configurable via <code>from_database_with_flush_interval</code>) and on drop, so a crash can lose approximately the most recent 64 stores. Deletes commit durably, so invalidations survive a crash.</li>
<li><code>StreamingManager</code> metadata now includes a blake3 body checksum, verified while streaming; corrupt entries error the stream and self-heal to a miss. Existing streaming cache entries from earlier alphas fail to decode and are treated as misses (re-fetched once).</li>
<li><code>StreamingBody::from_file_with_size</code> now streams exactly <code>size</code> bytes and errors on a shorter file; it previously read to end of file, treating <code>size</code> as a hint.</li>
<li><code>StreamingManager</code> no longer prewarms its in-memory metadata cache at startup; hydration is lazy, so <code>entry_count()</code> reads 0 after a restart until keys are accessed.</li>
<li>Non-cacheable responses on the streaming path are now passed through without buffering (<code>StreamingManager::Body</code> is <code>StreamingBody&lt;UnsyncBoxBody&lt;Bytes, StreamingError&gt;&gt;</code>). Upstream body failures on that path surface as stream errors while the body is read, not as a middleware error before the response is returned.</li>
<li>Concurrent operations on the same cache key in <code>StreamingManager</code> are serialized with per-key locks, so racing gets/puts can no longer drop freshly written entries.</li>
<li><code>StreamingManager::put</code> writes the response body to disk one frame at a time instead of collecting it into memory first, so peak memory no longer scales with response size. It returns a body backed by the committed on-disk entry rather than an in-memory copy.</li>
<li>Responses larger than <code>max_body_size</code> are no longer an error: caching is declined and the response is served in full, uncached. A declared <code>Content-Length</code> over the limit skips writing to disk entirely. HEAD responses are exempt, since their <code>Content-Length</code> describes the entity rather than the empty body.</li>
<li>Responses whose received length does not match their declared <code>Content-Length</code> are served but not cached, so a truncated response is never stored (RFC 9111 s3.3). HEAD responses are exempt.</li>
<li>Hop-by-hop headers are no longer written into cached entries. This covers <code>connection</code>, <code>transfer-encoding</code>, the rest of the RFC 9111 s3.1 set, and any field names listed in a <code>Connection</code> header. Live responses are unaffected.</li>
<li>304 revalidation on the streaming path updates the stored metadata in place instead of re-reading and rewriting the body file, so revalidating a large entry no longer costs a full read and write.</li>
<li><code>StreamingCacheManager::put</code> documents when a stored entry becomes visible: at or after the returned body has been fully consumed. The bundled <code>StreamingManager</code> makes it visible before <code>put</code> returns, but implementations are not required to.</li>
</ul>
<h2>[1.0.0-alpha.6] - 2026-04-17</h2>
<h3>Added</h3>
<ul>
<li><code>HttpStreamingCache::run</code> orchestrator for streaming cache operations, eliminating duplicated logic in downstream middleware crates</li>
<li><code>FetchRequest</code> enum for callback-based fetch dispatch in streaming paths</li>
<li><code>run_no_cache_from_parts</code> public method on <code>HttpCache</code> for cache-busting with pre-extracted request parts</li>
<li><code>CachedUserMetadata</code> for preserving user metadata through 304 re-cache operations</li>
<li><code>response_cache_mode_fn</code> evaluation in <code>conditional_fetch</code> 200 branch (both paths)</li>
</ul>
<h3>Changed</h3>
<ul>
<li><code>StreamingManager</code> storage backend replaced with an embedded <code>redb</code> database (for metadata) plus raw <code>tokio::fs</code> files (for response bodies), fronted by <code>moka</code> as an in-memory hot cache. This replaces the previous <code>cacache</code>-based backend. Overwrites atomically replace prior content; there is no longer any background eviction-driven disk cleanup. Disk state is managed explicitly by <code>put</code>, <code>delete</code>, and <code>clear</code>. The on-disk format is not compatible with previous releases — users upgrading should delete the <code>metadata.redb</code> file, the <code>bodies/</code> subdirectory, and the <code>tmp/</code> subdirectory from their cache directory before the first run on this version.</li>
<li>Only one <code>StreamingManager</code> instance may point at a given cache directory at a time (enforced via the database file lock on <code>metadata.redb</code>). Attempting to construct a second instance while another is alive returns an error.</li>
<li><code>StreamingManager::entry_count()</code> now returns the count of entries currently warm in the in-memory hot cache, not the total number of entries persisted on disk. When the cache has fewer entries than the configured capacity, this equals the previous value; once capacity is exceeded, cold entries remain reachable via <code>get</code> but are not counted here.</li>
<li>Replaced <code>async_trait</code> with native async functions in traits (RPITIT) for <code>CacheManager</code>, <code>StreamingCacheManager</code>, and <code>Middleware</code> traits</li>
<li><code>CacheAwareRateLimiter::until_key_ready</code> now returns <code>Pin&lt;Box&lt;dyn Future&gt;&gt;</code> for dyn-compatibility</li>
<li>Removed <code>async-trait</code> dependency</li>
<li>Cache deserialization failure logs lowered from <code>warn</code> to <code>debug</code> level</li>
<li><code>modify_response_before_caching</code> is now <code>pub</code> on <code>HttpCacheOptions</code></li>
</ul>
<h3>Fixed</h3>
<ul>
<li><code>StreamingManager</code> now persists cache entries across service restarts (#159). Previously the key→content mapping was only kept in memory, so cached bodies on disk became unreachable orphans after restart.</li>
<li>Multi-valued response headers (e.g. <code>Set-Cookie</code>, <code>Via</code>, <code>Link</code>) are now preserved correctly when merging headers from revalidation responses. Previously <code>HttpResponse::update_headers</code>, the streaming <code>conditional_fetch</code> Fresh/304 merges, and the streaming <code>handle_not_modified</code> path all lost or duplicated values for same-named headers.</li>
<li>RFC 7234 s4.4: cache invalidation now gated on successful response status (2xx/3xx) for unsafe methods</li>
<li>RFC 7234 s4.4: HEAD cache entries now invalidated alongside GET entries</li>
<li><code>conditional_fetch</code> 200 branch now checks <code>should_cache_response</code> before caching</li>
<li>Streaming path now respects <code>matches</code> flag from <code>BeforeRequest::Stale</code></li>
<li>Streaming path now handles all cache modes (NoCache, ForceCache, OnlyIfCached, IgnoreRules)</li>
<li>Streaming 304 path preserves original user metadata instead of regenerating</li>
<li><code>modify_response_before_caching</code> now called at correct point in streaming cache paths</li>
<li>Cache status headers set in correct order relative to <code>modify_response</code> and <code>manager.put</code></li>
<li>Warning header cleanup (1xx removal) on cached responses in streaming path</li>
<li>5xx stale fallback with Warning 111 in streaming conditional fetch</li>
<li><code>OnlyIfCached</code> 504 body now empty (consistent between streaming and non-streaming)</li>
</ul>
<h2>[1.0.0-alpha.5] - 2026-02-17</h2>
<h3>Added</h3>
<ul>
<li><code>url-standard</code> feature flag as the new default URL implementation using the <code>url</code> crate</li>
<li>Transparent bincode-to-postcard migration in <code>CACacheManager</code> and <code>MokaManager</code> (reads legacy bincode data, writes postcard)</li>
</ul>
<h3>Changed</h3>
<ul>
<li><code>StreamingManager</code> rewritten to use cacache for disk storage with moka for metadata tracking and TinyLFU eviction</li>
<li>MSRV bumped from 1.85.0 to 1.88.0</li>
<li>Updated <code>http-cache-semantics</code> from 2.1.0 to 3.0.0</li>
<li>Updated <code>foyer</code> to 0.22.3</li>
<li>Updated <code>rand</code> to 0.10.0</li>
<li><code>url</code> crate is now optional behind <code>url-standard</code> feature (enabled by default)</li>
<li>Removed <code>runtime</code> module, streaming no longer requires runtime-specific code</li>
</ul>
<h3>Fixed</h3>
<ul>
<li>Metadata handling with bincode serialization</li>
<li><code>CacheMode</code> enum variant matching logic</li>
<li>Feature flag gates for doc tests</li>
</ul>
<h2>[1.0.0-alpha.4] - 2026-01-19</h2>
<h3>Added</h3>
<ul>
<li><code>empty_body</code> method to <code>StreamingCacheManager</code> trait for creating empty body responses</li>
<li><code>get_ref_count</code> method to <code>ContentRefCounter</code> for non-mutating reference count checks</li>
<li><code>FoyerManager</code> cache backend using foyer for hybrid in-memory and disk caching</li>
<li><code>manager-foyer</code> feature flag for enabling <code>FoyerManager</code></li>
<li><code>manager-cacache-bincode</code> and <code>manager-moka-bincode</code> feature flags for legacy bincode serialization</li>
<li><code>url-ada</code> feature flag for using WHATWG-compliant ada-url as an alternative to the url crate</li>
<li><code>url_parse</code>, <code>url_set_path</code>, <code>url_set_query</code>, <code>url_hostname</code>, <code>url_host_str</code> helper functions for URL operations</li>
</ul>
<h3>Changed</h3>
<ul>
<li><code>StreamingManager</code> now wraps <code>ContentRefCounter</code> in <code>Arc</code> to ensure all clones share the same state</li>
<li>Atomic operations in streaming cache now use proper memory ordering (<code>Acquire</code>/<code>Release</code>/<code>AcqRel</code>) instead of <code>Relaxed</code></li>
<li>Default serialization format changed from bincode to postcard (cache data incompatible with previous versions)</li>
<li>Removed <code>cacache-smol</code> and <code>cacache-tokio</code> features in favor of tokio-only runtime</li>
<li><code>cacache</code> dependency now uses <code>tokio-runtime</code> by default</li>
<li>Removed async-std from dependency tree</li>
</ul>
<h3>Fixed</h3>
<ul>
<li>Race condition in <code>remove_ref</code> using atomic <code>compare_exchange</code> loop to prevent TOCTOU bugs</li>
<li>Cache size and entry count divergence when <code>StreamingManager</code> is cloned</li>
<li>Memory leak in <code>delete</code> where reference count was decremented but not restored on non-orphaned content</li>
<li>Race condition in <code>delete</code> by using non-mutating <code>get_ref_count</code> instead of remove/add pattern</li>
</ul>
<h2>[1.0.0-alpha.3] - 2026-01-18</h2>
<h3>Added</h3>
<ul>
<li><code>modify_response</code> field to <code>HttpCacheOptions</code> for modifying responses before storing in cache</li>
<li><code>http-headers-compat</code> feature flag for header compatibility options</li>
<li><code>metadata</code> field to <code>HttpResponse</code> for storing arbitrary data with cached responses</li>
<li><code>metadata_provider</code> function to <code>HttpCacheOptions</code> for computing metadata on cache store</li>
</ul>
<h3>Changed</h3>
<ul>
<li>MSRV is now 1.85.0</li>
</ul>
<h3>Fixed</h3>
<ul>
<li>Serialize all header values instead of just the first value per header name</li>
<li><code>HttpHeaders</code> serialization and insert behavior for bincode compatibility</li>
<li>Preserve all header values sharing the same name</li>
</ul>
<h2>[1.0.0-alpha.2] - 2025-08-24</h2>
<h3>Added</h3>
<ul>
<li><code>max_ttl</code> field to <code>HttpCacheOptions</code> for controlling maximum cache duration</li>
<li>Support for <code>Duration</code> type in <code>max_ttl</code> field for better ergonomics and type safety</li>
<li>Cache duration limiting functionality that overrides longer server-specified durations while respecting shorter ones</li>
<li>Enhanced cache expiration control for <code>CacheMode::IgnoreRules</code> mode</li>
<li><code>rate_limiter</code> field to <code>HttpCacheOptions</code> for cache-aware rate limiting that only applies on cache misses</li>
<li><code>CacheAwareRateLimiter</code> trait for implementing rate limiting strategies</li>
<li><code>DomainRateLimiter</code> for per-domain rate limiting using governor</li>
<li><code>DirectRateLimiter</code> for global rate limiting using governor</li>
<li>New <code>rate-limiting</code> feature flag for optional rate limiting functionality</li>
<li>Rate limiting support for streaming cache operations with seamless integration</li>
<li>Simple LRU eviction policy for the <code>StreamingManager</code> with configurable size and entry limits</li>
<li>Multi-runtime async support (tokio/smol) with <code>RwLock</code> for better async performance</li>
<li>Content deduplication using Blake3 hashing for efficient storage</li>
<li>Atomic file operations using temporary files and rename for safe concurrent access</li>
<li>Configurable streaming buffer size for optimal streaming performance</li>
<li>Lock-free reference counting using DashMap for concurrent access</li>
<li>LRU cache implementation using the <code>lru</code> crate</li>
</ul>
<h3>Changed</h3>
<ul>
<li><code>max_ttl</code> implementation automatically enforces cache duration limits by modifying response cache-control headers</li>
<li>Documentation updated with comprehensive examples for <code>max_ttl</code> usage across all cache modes</li>
<li><code>StreamingCacheConfig</code> simplified to essential configuration options:
<ul>
<li><code>max_cache_size</code>: Optional cache size limit for LRU eviction</li>
<li><code>max_entries</code>: Optional entry count limit for LRU eviction</li>
<li><code>streaming_buffer_size</code>: Buffer size for streaming operations (default: 8192)</li>
</ul>
</li>
<li>Enhanced error types and handling for streaming cache operations</li>
<li>Simplified <code>StreamingManager</code> implementation focused on core functionality and maintainability</li>
<li>Removed unused background cleanup and persistent reference counting infrastructure for cleaner codebase</li>
<li>Improved async compatibility across tokio and smol runtimes</li>
<li>Upgraded concurrent data structures to use DashMap and LRU cache</li>
<li>Replaced custom implementations with established library solutions</li>
</ul>
<h3>Fixed</h3>
<ul>
<li>Race conditions in reference counting during concurrent access</li>
<li>Resource leaks in streaming cache operations when metadata write fails</li>
<li>Unsafe unwrap operations in cache entry manipulation</li>
<li>Inefficient URL construction replaced with safer url crate methods</li>
<li>Improved error handling and recovery in streaming operations</li>
</ul>
<h2>[1.0.0-alpha.1] - 2025-07-27</h2>
<h3>Added</h3>
<ul>
<li>New streaming cache architecture for handling large HTTP responses without buffering entirely in memory</li>
<li><code>StreamingCacheManager</code> trait for streaming-aware cache backends</li>
<li><code>HttpCacheStreamInterface</code> trait for composable streaming middleware patterns</li>
<li><code>HttpStreamingCache</code> struct for managing streaming cache operations</li>
<li><code>StreamingManager</code> implementation using file-based storage</li>
<li><code>StreamingBody</code> type for handling both buffered and streaming scenarios</li>
<li><code>CacheAnalysis</code> struct for better separation of cache decision logic</li>
<li><code>response_cache_mode_fn</code> field to <code>HttpCacheOptions</code> for per-response cache mode overrides</li>
<li>New streaming feature flags: <code>streaming</code>, <code>streaming-tokio</code>, <code>streaming-smol</code></li>
</ul>
<h3>Changed</h3>
<ul>
<li>Refactored <code>Middleware</code> trait for better composability</li>
<li>Cache manager interfaces now support both buffered and streaming operations</li>
<li>Enhanced separation of concerns with discrete analysis/lookup/processing steps</li>
<li>Renamed <code>cacache-async-std</code> feature to <code>cacache-smol</code> for consistency</li>
<li>MSRV updated to 1.82.0</li>
</ul>
<h2>[0.21.0] - 2025-06-25</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>remove_opts</code> field to <code>CACacheManager</code> struct. This field is an instance of <code>cacache::RemoveOpts</code> that allows for customization of the removal options when deleting items from the cache.</p>
</li>
<li>
<p>MSRV is now 1.82.0</p>
</li>
</ul>
<h2>[0.20.1] - 2025-01-30</h2>
<h3>Changed</h3>
<ul>
<li>
<p>Fixed missing implementation of CacheMode::Reload variant logic.</p>
</li>
<li>
<p>MSRV is now 1.81.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.85]</li>
<li>cacache [13.1.0]</li>
<li>httpdate [1.0.2]</li>
<li>moka [0.12.10]</li>
<li>serde [1.0.217]</li>
<li>url [2.5.4]</li>
</ul>
</li>
</ul>
<h2>[0.20.0] - 2024-11-12</h2>
<h3>Added</h3>
<ul>
<li><code>cache_status_headers</code> field to <code>HttpCacheOptions</code> struct. This field is a boolean that determines if the cache status headers should be added to the response.</li>
</ul>
<h2>[0.19.0] - 2024-04-10</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>cacache [13.0.0]</li>
<li>http [1.1.0]</li>
<li>http-cache-semantics [2.1.0]</li>
</ul>
</li>
</ul>
<h2>[0.18.0] - 2024-01-15</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>overridden_cache_mode</code> method to <code>Middleware</code> trait. This method allows for overriding any cache mode set in the configuration, including <code>cache_mode_fn</code>.</p>
</li>
<li>
<p>Derive <code>Default</code> for the <code>CacheMode</code> enum with the mode <code>Default</code> selected to be used.</p>
</li>
</ul>
<h2>[0.17.0] - 2023-11-01</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>cache_mode_fn</code> field to <code>HttpCacheOptions</code> struct. This is a closure that takes a <code>&amp;http::request::Parts</code> and returns a <code>CacheMode</code> enum variant. This allows for the overriding of cache mode on a per-request basis.</p>
</li>
<li>
<p><code>cache_bust</code> field to <code>HttpCacheOptions</code> struct. This is a closure that takes <code>http::request::Parts</code>, <code>Option&lt;CacheKey&gt;</code>, the default cache key (<code>&amp;str</code>) and returns <code>Vec&lt;String&gt;</code> of keys to bust the cache for.</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>cacache [12.0.0]</li>
</ul>
</li>
</ul>
<h2>[0.16.0] - 2023-09-28</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>can_cache_request</code> method to <code>HttpCache</code> struct. This can be used by client implementations to determine if the request should be cached.</p>
</li>
<li>
<p><code>run_no_cache</code> method to <code>HttpCache</code> struct. This should be run by client implementations if the request is determined to not be cached.</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>MSRV is now 1.67.1</li>
</ul>
<h2>[0.15.0] - 2023-09-26</h2>
<h3>Added</h3>
<ul>
<li><code>IgnoreRules</code> variant to the <code>CacheMode</code> enum. This mode will ignore the HTTP headers and always store a response given it was a 200 response. It will also ignore the staleness when retrieving a response from the cache, so expiration of the cached response will need to be handled manually. If there was no cached response it will create a normal request, and will update the cache with the response.</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>moka [0.12.0]</li>
</ul>
</li>
</ul>
<h2>[0.14.0] - 2023-07-28</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>cacache-async-std</code> feature, which enables <code>async_std</code> runtime support in the <code>cacache</code> backend manager. This feature is enabled by default.</p>
</li>
<li>
<p><code>cacache-tokio</code> feature, which enables <code>tokio</code> runtime support in the <code>cacache</code> backend manager. This feature is disabled by default.</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>async-std [1.12.0]</li>
<li>async-trait [0.1.72]</li>
<li>serde [1.0.178]</li>
<li>tokio [1.29.1]</li>
</ul>
</li>
</ul>
<h2>[0.13.0] - 2023-07-19</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>CacheKey</code> type, a closure that takes [<code>http::request::Parts</code>] and returns a [<code>String</code>].</p>
</li>
<li>
<p><code>HttpCacheOptions</code> struct that contains the cache key (<code>CacheKey</code>) and the cache options (<code>CacheOptions</code>).</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>
<p><code>CacheManager</code> trait <code>get</code>, <code>put</code>, and <code>delete</code> methods now require a <code>cache_key</code> argument rather than <code>method</code> and <code>url</code> arguments. This allows for custom keys to be specified.</p>
</li>
<li>
<p>Both the <code>CACacheManager</code> trait and <code>MokaManager</code> implementation have been updated to reflect the above change.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.71]</li>
<li>moka [0.11.2]</li>
<li>serde [1.0.171]</li>
</ul>
</li>
</ul>
<h2>[0.12.0] - 2023-06-05</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.66.1</p>
</li>
<li>
<p><code>CACacheManager</code> field <code>path</code> has changed to <code>std::path::PathBuf</code></p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>cacache [11.6.0]</li>
<li>moka [0.11.1]</li>
<li>serde [1.0.163]</li>
<li>url [2.4.0]</li>
</ul>
</li>
</ul>
<h2>[0.11.0] - 2023-03-29</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>BoxError</code> type alias for <code>Box&lt;dyn std::error::Error + Send + Sync&gt;</code>.</p>
</li>
<li>
<p><code>BadVersion</code> error type for unknown http versions.</p>
</li>
<li>
<p><code>BadHeader</code> error type for bad http header values.</p>
</li>
</ul>
<h3>Removed</h3>
<ul>
<li>
<p><code>CacheError</code> enum.</p>
</li>
<li>
<p>The following dependencies:</p>
<ul>
<li>anyhow</li>
<li>thiserror</li>
<li>miette</li>
</ul>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>
<p><code>CacheError</code> enum has been replaced in function by <code>Box&lt;dyn std::error::Error + Send + Sync&gt;</code>.</p>
</li>
<li>
<p><code>Result</code> typedef is now <code>std::result::Result&lt;T, BoxError&gt;</code>.</p>
</li>
<li>
<p><code>Error</code> type for the TryFrom implentation for the <code>HttpVersion</code> struct is now <code>BoxError</code> containing a <code>BadVersion</code> error.</p>
</li>
<li>
<p><code>CacheManager</code> trait <code>put</code> method now returns <code>Result&lt;(), BoxError&gt;</code>.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.68]</li>
<li>cacache [11.4.0]</li>
<li>moka [0.10.1]</li>
<li>serde [1.0.159]</li>
</ul>
</li>
</ul>
<h2>[0.10.1] - 2023-03-08</h2>
<h3>Changed</h3>
<ul>
<li>Set conditional check for <code>CacheError::Bincode</code> to <code>cfg(feature = "bincode")</code></li>
</ul>
<h2>[0.10.0] - 2023-03-08</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.63.0</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.66]</li>
<li>cacache [11.3.0]</li>
<li>serde [1.0.154]</li>
<li>thiserror [1.0.39]</li>
</ul>
</li>
</ul>
<h2>[0.9.2] - 2023-02-23</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>cacache [11.1.0]</li>
</ul>
</li>
</ul>
<h2>[0.9.1] - 2023-02-17</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http [0.2.9]</li>
</ul>
</li>
</ul>
<h2>[0.9.0] - 2023-02-16</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.62.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>moka [0.10.0]</li>
</ul>
</li>
</ul>
<h2>[0.8.0] - 2023-02-07</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.60.0</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>anyhow [1.0.69]</li>
<li>async-trait [0.1.64]</li>
<li>cacache [11.0.0]</li>
<li>miette [5.5.0]</li>
<li>moka [0.9.7]</li>
<li>serde [1.0.152]</li>
<li>thiserror [1.0.38]</li>
</ul>
</li>
</ul>
<h2>[0.7.2] - 2022-11-16</h2>
<ul>
<li>Added derive <code>Eq</code> to <code>HttpVersion</code> enum.</li>
</ul>
<h3>Changed</h3>
<h2>[0.7.1] - 2022-11-06</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>anyhow [1.0.66]</li>
<li>async-trait [0.1.58]</li>
<li>miette [5.4.1]</li>
<li>moka [0.9.6]</li>
<li>serde [1.0.147]</li>
<li>thiserror [1.0.37]</li>
<li>url [2.3.1]</li>
</ul>
</li>
</ul>
<h2>[0.7.0] - 2022-06-17</h2>
<h3>Changed</h3>
<ul>
<li>
<p>The <code>CacheManager</code> trait is now implemented directly against the <code>MokaManager</code> struct rather than <code>Arc&lt;MokaManager&gt;</code>. The Arc is now internal to the <code>MokaManager</code> struct as part of the <code>cache</code> field.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.56]</li>
<li>http [0.2.8]</li>
<li>miette [4.7.1]</li>
<li>moka [0.8.5]</li>
<li>serde [1.0.137]</li>
<li>thiserror [1.0.31]</li>
</ul>
</li>
</ul>
<h2>[0.6.5] - 2022-04-30</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http [0.2.7]</li>
</ul>
</li>
</ul>
<h2>[0.6.4] - 2022-04-26</h2>
<h3>Added</h3>
<ul>
<li>This changelog to keep a record of notable changes to the project.</li>
</ul>

  

        </main>
      </div>

      <footer>
        
          <a href="https://github.com/06chaynes/http-cache"><div class="github-icon" aria-hidden="true"></div></a>
        
        <span>
          http-cache, MIT OR Apache-2.0
        </span>
      </footer>
    </div>

    
    

    
  </body>
</html>