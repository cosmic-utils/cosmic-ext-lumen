app-title = Lumen
app-comment = Browse Lemmy communities and discussions
app-keywords = Lemmy;Fediverse;COSMIC;

## Navigation and app shell

inbox-unread = Inbox ({ $count })
all-posts = All posts
local = Local
subscribed = Subscribed
saved = Saved
inbox = Inbox
community-subscribers =
    { $count ->
        [one] { $count } subscriber
       *[other] { $count } subscribers
    }
search-placeholder = Search or paste a link
new-post = New post
accounts = Accounts
restoring-account = Restoring account...
sign-in = Sign in
search-results = Search results
profile = Profile
community = Community
load-images-automatically = Load images automatically
accounts-and-instances = Accounts and instances
browse-community = Browse community
unsubscribe = Unsubscribe
subscribe = Subscribe
subscription-cancel-pending = Cancel pending subscription
open-in-browser = Open in browser
loading-community = Loading community...
community-details = Community details

## Accounts and credentials

credential-worker-failed = Credential worker failed
auth-https-required = Login requires HTTPS (except localhost).
auth-no-token = No session token returned
session-revoke-failed = Removed locally; server session could not be revoked. Revoke it from Lemmy settings.
anonymous = Anonymous
remove-account = Remove account
working = Working...
add-account = Add account
username-or-email = Username or email
password = Password
auth-2fa-optional = 2FA code (optional)
instance-version-required = Requires Lemmy 0.19.x
session-expired = Session expired; log in again.
credentials-unavailable = Credential store unavailable: { $error }
credentials-restore-failed = Cannot restore session; unlock your credential store or log in again: { $error }
credentials-save-failed = Cannot save session in credential store: { $error }
credentials-remove-failed = Cannot remove saved session: { $error }
settings-dir-missing = Missing settings directory

## Communities

community-mismatch = The instance returned a different community.

## Feeds and search

reference-open-failed = Could not open this link or handle. { $error }
hot = Hot
new = New
active = Active
top-today = Top today
reference-not-found = The instance found no match for this link or handle.
person-mismatch = The instance returned a different person.
retry = Retry
search-as-keywords = Search as keywords
link-opening = Opening link or handle...
loading-comments = Loading comments...
loading-communities = Loading communities...
loading-people = Loading people...
loading-posts = Loading posts...
saved-no-comments = No saved comments.
saved-no-posts = No saved posts.
search-no-comments = No matching comments.
search-no-communities = No matching communities.
search-no-people = No matching people.
profile-no-comments = No comments on this profile.
profile-no-posts = No posts on this profile.
search-no-posts = No matching posts.
feed-empty = No posts in this feed.
instance-connect-prompt = Connect to an instance to start reading.
loading = Loading...
load-more-comments = Load more comments
load-more-communities = Load more communities
load-more-people = Load more people
load-more-posts = Load more posts
posts = Posts
comments = Comments
communities = Communities
people = People
all-communities = All communities
refresh-feed = Refresh feed
feed-spacing-comfortable = Comfortable feed spacing
feed-spacing-compact = Compact feed spacing
profile-open-browser = Open profile in browser

## Inbox

refresh-inbox = Refresh inbox
loading-inbox = Loading inbox...
inbox-empty = No replies or mentions.
inbox-refresh-prompt = Refresh to load your inbox.
mark-as-unread = Mark as unread
mark-as-read = Mark as read
opening-discussion = Opening discussion...
load-more = Load more
inbox-read-unconfirmed = The instance did not confirm the requested read status.
inbox-reply = { $author } replied · { $age }
inbox-mention = { $author } mentioned you · { $age }
inbox-unread-status = Unread
inbox-notice-unread = { $notice } · { inbox-unread-status }

## Instance connection

instance-version-unsupported = This instance runs Lemmy { $version }. This reader currently supports 0.19.x (API v3).
settings-load-failed = Could not load settings: { $error }
instance-save-failed = Connected, but could not save the instance: { $error }
connected-to = Connected to { $name }
connecting = Connecting...
connect = Connect
instance = Instance
instance-url-placeholder = lemmy.world or https://your.instance

## Discussion and comment actions

comment-post-mismatch = The instance returned a comment for a different post.
reply-parent-mismatch = The instance returned a reply to a different comment.
post-mismatch = The instance returned a different post.
post-status-unconfirmed = The instance did not confirm the requested post status.
comment-mismatch = The instance returned a different comment.
view-image = View image
write-a-comment = Write a comment
post-comment = Post comment
write-a-reply = Write a reply
post-reply = Post reply
edit-comment = Edit comment
save-changes = Save changes
saving = Saving...
posting = Posting...
cancel = Cancel
discussion-select-post = Select a post to open its discussion.
back-to-posts = Back to posts
discussion = Discussion
feed-beside-discussion = Show feed beside discussion
focus-reading = Focus reading
discussion-open-browser = Open discussion in browser
comment-in-context = Comment in context
loading-comment = Loading comment...
post-deleted = This post was deleted.
post-removed = This post was removed.
save = Save
cross-post = Cross-post
restore-post = Restore post
edit-post = Edit post
delete-post = Delete post
delete-this-post = Delete this post?
delete = Delete
updating-post = Updating post...
top = Top
old = Old
close = Close
add-comment = Add comment
discussion-locked = This discussion is locked.
retry-comments = Retry comments
comments-empty = No comments yet.
expand-comment = Expand comment
collapse-comment = Collapse comment
unsave-comment = Unsave comment
save-comment = Save comment
reply = Reply
restore-comment = Restore comment
delete-comment = Delete comment
comment-parent = Jump to parent comment
copy-comment-permalink = Copy comment permalink
clear-upvote = Clear upvote
upvote = Upvote
clear-downvote = Clear downvote
downvote = Downvote

## Post composer and uploads

upload-picker-failed = Could not open the image picker: { $error }
image-read-failed = Could not read image: { $error }
image-choose = Choose an image
images = Images
upload-choose-file = Choose a local image file.
upload-too-large = Choose an image smaller than 12 MiB.
upload-unsupported-format = Choose a JPEG, PNG, GIF, or WebP image.
image-reader-failed = Image reader did not finish.
cross-post-attribution = Cross-posted from { $link }
draft-open-failed = Could not open draft: { $error }
draft-not-found = Could not locate draft: { $error }
draft-save-failed = Could not save draft: { $error }
draft-published-remove-failed = Could not remove the published draft: { $error }
draft-discard-failed = Could not discard draft: { $error }
link-invalid = Enter a valid HTTP or HTTPS link.
loading-image = Loading image...
image-preview-failed = Image preview unavailable.
other-community = Other community...
community-choose = Choose a community
community-handle-placeholder = Community name or name@instance
write-in-markdown = Write in Markdown
post-title = Post title
title = Title
post-url-placeholder = Link or image URL (optional)
uploading = Uploading...
replace-image = Replace image...
upload-image = Upload image...
remove-image = Remove image
image-description = Describe the image
image-description-optional = Image description (optional)
body = Body
untitled-post = Untitled post
back-to-editing = Back to editing
preview-post = Preview post
publishing = Publishing...
publish = Publish
discard-draft = Discard draft
cross-post-choose-community = Choose a different community for the cross-post.
post-community-mismatch = The instance returned a post in a different community.
cross-post-original = the original discussion

## Shared UI and errors

upload-failed = Could not upload image: { $error }
upload-http-error = The instance rejected the image (HTTP { $status }).
request-http-error = The instance rejected the request (HTTP { $status }).
api-response-invalid = Unexpected Lemmy API v3 response: { $error }
instance-connection-error = Could not reach { $instance }: { $error }
instance-http-error = The instance returned HTTP { $status }. Please retry.
upload-sign-in-required = Sign in to upload images.
upload-response-invalid = Unexpected image upload response.
upload-missing-filename = The instance did not return an image filename.
image-service-url-invalid = Invalid image service URL.
instance-unreachable = Could not reach instance
api-response-unexpected = Unexpected API response
instance-access-denied = This instance requires login or denied this request.
api-not-found = Lemmy API v3 was not found. This reader supports Lemmy 0.19 instances.
instance-rate-limited = This instance is rate limiting requests. Wait a moment and retry.
instance-prompt = Enter a Lemmy instance, such as lemmy.world.
instance-url-invalid = Enter a valid instance URL.
instance-scheme-required = Use an HTTP or HTTPS instance URL.
instance-credentials-not-allowed = Instance URLs must not contain credentials.
instance-url-root = Enter the instance root URL, without a community or post path.
browser-open-failed = Could not open browser: { $error }
link-scheme-unsupported = Only HTTP and HTTPS links can be opened.
settings-dir-not-found = Could not locate the settings directory
image-settings-load-failed = Could not load image settings: { $error }
image-settings-save-failed = Could not save image settings: { $error }
image-zoom-percent = { $percent }%
image = Image
zoom-out = Zoom out
fit = Fit
zoom-in = Zoom in
image-display-failed = This image could not be displayed.
age-minutes = { $count }m
age-hours = { $count }h
age-days = { $count }d
now = now
feed-comment-metadata = { $author } · { $community }
comment-deleted = [deleted]
comment-removed = [removed]
