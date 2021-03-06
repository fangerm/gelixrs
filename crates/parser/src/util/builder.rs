use crate::{Node, NodeOrToken, NodeVec, Token};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::rc::Rc;
use syntax::kind::SyntaxKind;

#[repr(transparent)]
pub struct NodeBuilder {
    nodes: Vec<WorkNode>,
}

impl NodeBuilder {
    pub fn start_node(&mut self, kind: SyntaxKind) {
        let pos = self.nodes.last().map(|n| n.end).unwrap_or(0);
        self.nodes.push(WorkNode {
            children: SmallVec::new(),
            kind,
            start: pos,
            end: pos,
        })
    }

    pub fn end_node(&mut self) {
        let node = self.nodes.pop().expect("No node?");
        let mut current = self.current();
        current.end = node.end;
        current.children.push(NodeOrToken::Node(node.into_node()));
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            node: self.nodes.len(),
            child_count: self.nodes.last().unwrap().children.len(),
            start: self.nodes.last().unwrap().end,
        }
    }

    pub fn start_node_at(&mut self, kind: SyntaxKind, loc: Checkpoint) {
        let parent = &mut self.nodes[loc.node - 1];
        let mut children: NodeVec =
            SmallVec::with_capacity(parent.children.len() - loc.child_count);

        let mut last_node_end = loc.start;
        let mut tokens_end = 0;
        for pop in self.nodes[loc.node - 1].children.drain(loc.child_count..) {
            match &pop {
                NodeOrToken::Node(node) => {
                    last_node_end = node.text_range().end;
                    tokens_end = 0;
                }
                NodeOrToken::Token(tok) => tokens_end += tok.text().len() as u32,
            }
            children.push(pop);
        }

        let end = last_node_end + tokens_end;
        self.nodes.insert(
            loc.node,
            WorkNode {
                children,
                kind,
                start: loc.start,
                end,
            },
        )
    }

    pub fn token(&mut self, kind: SyntaxKind, text: SmolStr) {
        let mut current = self.current();
        current.end += text.len() as u32;
        current
            .children
            .push(NodeOrToken::Token(Token::new(kind, text)))
    }

    fn current(&mut self) -> &mut WorkNode {
        self.nodes.last_mut().unwrap()
    }

    pub fn finish(mut self) -> Node {
        self.nodes.pop().expect("Missing root node?").into_node()
    }

    pub fn new() -> Self {
        Self {
            nodes: vec![WorkNode {
                children: SmallVec::new(),
                kind: SyntaxKind::Root,
                start: 0,
                end: 0,
            }],
        }
    }
}

#[derive(Debug)]
struct WorkNode {
    children: NodeVec,
    kind: SyntaxKind,
    start: u32,
    end: u32,
}

impl WorkNode {
    pub fn into_node(self) -> Node {
        Node::new(Rc::new(self.children), self.kind, self.start..self.end)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Checkpoint {
    node: usize,
    child_count: usize,
    start: u32,
}
