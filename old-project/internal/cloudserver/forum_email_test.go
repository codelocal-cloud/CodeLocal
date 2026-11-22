package cloudserver

import (
	"strings"
	"testing"

	"github.com/0xmarkhydra/codelocal/internal/cloud"
)

func TestForumAdminTopicEmailIncludesAuthorIdentity(t *testing.T) {
	topic := cloud.ForumTopic{ID: "forum_1", Kind: "bug", Title: "MCP cannot connect", Body: "Connection fails after login", Status: "open", AuthorEmail: "reporter@example.com"}
	_, text, htmlBody := forumAdminTopicCreatedEmail(topic)
	if !strings.Contains(text, topic.AuthorEmail) || !strings.Contains(htmlBody, topic.AuthorEmail) {
		t.Fatalf("admin notification must include the topic author's real email")
	}
}

func TestForumTopicOwnerCommentEmailHidesNormalUserEmail(t *testing.T) {
	t.Setenv("CODELOCAL_ADMIN_EMAILS", "admin@codelocal.cloud")
	topic := cloud.ForumTopic{ID: "forum_1", Title: "Need help", AuthorEmail: "owner@example.com"}
	comment := cloud.ForumComment{ID: "forumc_1", AuthorEmail: "commenter@example.com", Body: "I can reproduce this."}
	_, text, htmlBody := forumTopicOwnerCommentEmail(topic, comment)
	combined := text + htmlBody
	if strings.Contains(combined, comment.AuthorEmail) {
		t.Fatalf("topic owner notification leaked commenter email: %q", comment.AuthorEmail)
	}
	if !strings.Contains(combined, "Community member") {
		t.Fatal("normal commenter should be identified only as Community member")
	}
}

func TestForumTopicOwnerCommentEmailLabelsAdminWithoutLeakingEmail(t *testing.T) {
	t.Setenv("CODELOCAL_ADMIN_EMAILS", "admin@codelocal.cloud")
	topic := cloud.ForumTopic{ID: "forum_1", Title: "Need help", AuthorEmail: "owner@example.com"}
	comment := cloud.ForumComment{ID: "forumc_1", AuthorEmail: "admin@codelocal.cloud", Body: "We are reviewing this."}
	_, text, htmlBody := forumTopicOwnerCommentEmail(topic, comment)
	combined := text + htmlBody
	if strings.Contains(combined, comment.AuthorEmail) {
		t.Fatalf("topic owner notification leaked admin email: %q", comment.AuthorEmail)
	}
	if !strings.Contains(combined, "CodeLocal Team") {
		t.Fatal("admin reply should be identified as CodeLocal Team")
	}
}

func TestForumStatusEmailIncludesResolutionWithoutOtherUserIdentity(t *testing.T) {
	topic := cloud.ForumTopic{ID: "forum_1", Title: "Broken plugin", Status: "resolved", ResolutionNote: "Fixed in the latest deployment."}
	subject, text, htmlBody := forumTopicOwnerStatusEmail(topic)
	if !strings.Contains(strings.ToLower(subject), "resolved") {
		t.Fatalf("resolved topic subject should say resolved: %q", subject)
	}
	if !strings.Contains(text+htmlBody, topic.ResolutionNote) {
		t.Fatal("resolution note should be included in owner notification")
	}
}
