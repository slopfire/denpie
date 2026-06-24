pub(crate) const BASE_CARD_SELECT: &str = "SELECT t.id,
       top.name AS topic_name,
       COALESCE(NULLIF(TRIM(top.icon_id), ''), 'lucide:tag') AS topic_icon,
       top.color_hue,
       COALESCE(t.title, '') AS title,
       t.full_content,
       t.compressed_content,
       COALESCE(CAST(t.created_at AS TEXT), '') AS created_at,
       top.tipcard_type,
       COALESCE(r.status, CASE WHEN top.tipcard_type = 'custom_tip' THEN 'custom' ELSE 'active' END) AS status,
       COALESCE(CAST(r.next_review_at AS TEXT), '') AS next_review_at,
       COALESCE(r.state_data, '') AS state_data,
       COALESCE(r.repeats, 0) AS repeats,
       COALESCE(t.pinned, 0) AS pinned";

pub(crate) const CARD_FROM_JOINS: &str = "FROM tipcards t
JOIN topics top ON t.topic_id = top.id
LEFT JOIN review_states r ON r.card_id = t.id";

pub(crate) const FLOW_FROM_JOINS: &str = "FROM tipcards t
JOIN topics top ON t.topic_id = top.id
LEFT JOIN review_states r ON r.card_id = t.id
LEFT JOIN tipcard_images img ON img.card_id = t.id AND img.user_id = t.user_id";

pub(crate) const IMAGE_SELECT: &str =
    "SELECT id, position, storage_path, mime_type, byte_size FROM tipcard_images";

pub(crate) const SCHEDULED_SELECT: &str = "SELECT t.id,
       t.full_content,
       t.compressed_content,
       COALESCE(t.pinned, 0) AS pinned,
       COALESCE(t.image_data, '[]') AS image_data
FROM tipcards t";
